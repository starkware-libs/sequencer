use std::collections::HashMap;

use async_trait::async_trait;
use blockifier::state::cached_state::StateMaps;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_os::io::os_input::{CommitmentInfo, StateCommitmentInfos};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::{
    ConfirmedBlockId,
    ContractStorageKeys,
    Felt,
    StorageProof as RpcStorageProof,
};

use crate::errors::ProofProviderError;
use crate::virtual_block_executor::VirtualBlockExecutionData;

/// Provides Patricia Merkle proofs for the initial state used in transaction execution.
///
/// This trait abstracts the retrieval of storage proofs, which are essential for OS input
/// generation. The proofs allow the OS to verify that the initial state values (read during
/// execution) are consistent with the global state commitment (Patricia root).
///
/// The returned `StorageProofs` contains:
/// - `proof_state`: The ambient state values (nonces, class hashes) discovered in the proof.
/// - `commitment_infos`: The Patricia Merkle proof nodes for contracts, classes, and storage tries.
#[async_trait]
pub trait StorageProofProvider {
    async fn get_storage_proofs(
        &self,
        block_number: BlockNumber,
        execution_data: &VirtualBlockExecutionData,
    ) -> Result<StorageProofs, ProofProviderError>;
}

/// Query parameters for fetching storage proofs from RPC.
pub struct RpcStorageProofsQuery {
    pub class_hashes: Vec<Felt>,
    pub contract_addresses: Vec<ContractAddress>,
    pub contract_storage_keys: Vec<ContractStorageKeys>,
}

/// Complete OS input data built from RPC proofs.
pub struct StorageProofs {
    /// State information discovered in the Patricia proof (nonces, class hashes)
    /// that might not have been explicitly read during transaction execution.
    /// This data is required by the OS to verify the contract state leaves.
    pub proof_state: StateMaps,
    pub commitment_infos: StateCommitmentInfos,
}

/// Wrapper around `JsonRpcClient` for fetching storage proofs.
pub struct RpcStorageProofsProvider(pub JsonRpcClient<HttpTransport>);

impl RpcStorageProofsProvider {
    pub fn new(rpc_url: url::Url) -> Self {
        let transport = HttpTransport::new(rpc_url);
        let client = JsonRpcClient::new(transport);
        Self(client)
    }

    /// Converts RPC merkle nodes (hash → MerkleNode mapping) to a PreimageMap.
    fn rpc_nodes_to_preimage_map<S: std::hash::BuildHasher>(
        nodes: &indexmap::IndexMap<Felt, starknet_rust_core::types::MerkleNode, S>,
    ) -> PreimageMap {
        nodes.iter().map(|(hash, node)| (HashOutput(*hash), Preimage::from(node))).collect()
    }

    /// Extract query parameters from the execution data.
    pub fn prepare_query(execution_data: &VirtualBlockExecutionData) -> RpcStorageProofsQuery {
        let class_hashes: Vec<Felt> =
            execution_data.executed_class_hashes.iter().map(|ch| ch.0).collect();

        let initial_reads = &execution_data.initial_reads;
        let contract_addresses: Vec<ContractAddress> =
            initial_reads.get_contract_addresses().into_iter().collect();

        // Storage keys grouped by contract address.
        let mut storage_by_address: HashMap<ContractAddress, Vec<Felt>> = HashMap::new();
        for (address, key) in initial_reads.storage.keys() {
            storage_by_address.entry(*address).or_default().push(*key.0.key());
        }

        let contract_storage_keys: Vec<ContractStorageKeys> = contract_addresses
            .iter()
            .map(|address| ContractStorageKeys {
                contract_address: *address.0.key(),
                storage_keys: storage_by_address.get(address).cloned().unwrap_or_default(),
            })
            .collect();

        RpcStorageProofsQuery { class_hashes, contract_addresses, contract_storage_keys }
    }

    /// Fetch storage proofs from RPC.
    ///
    /// # RPC Order Guarantee
    ///
    /// This function relies on the Starknet JSON-RPC `get_storage_proof` method returning data
    /// in the same order as the input arrays:
    /// - `contract_leaves_data` matches the order of `contract_addresses`
    /// - `contracts_storage_proofs` matches the order of `contract_storage_keys`
    ///
    /// This is a standard API contract for batched requests. Validation is performed in
    /// `to_storage_proofs` to detect any violations of this assumption.
    pub async fn fetch_proofs(
        &self,
        block_number: BlockNumber,
        query: &RpcStorageProofsQuery,
    ) -> Result<RpcStorageProof, ProofProviderError> {
        let block_id = ConfirmedBlockId::Number(block_number.0);
        let contract_addresses: Vec<Felt> =
            query.contract_addresses.iter().map(|a| *a.0.key()).collect();

        let storage_proof = self
            .0
            .get_storage_proof(
                block_id,
                &query.class_hashes,
                &contract_addresses,
                &query.contract_storage_keys,
            )
            .await?;

        Ok(storage_proof)
    }

    /// Converts an RPC storage proof response to OS input format.
    ///
    /// # Validation
    ///
    /// This function validates that the RPC response arrays match the expected lengths from
    /// the query, ensuring the RPC provider returned data in the correct order.
    pub fn to_storage_proofs(
        rpc_proof: &RpcStorageProof,
        query: &RpcStorageProofsQuery,
    ) -> Result<StorageProofs, ProofProviderError> {
        // Validate that contract_leaves_data matches contract_addresses length.
        let leaves_len = rpc_proof.contracts_proof.contract_leaves_data.len();
        let addresses_len = query.contract_addresses.len();
        if leaves_len != addresses_len {
            return Err(ProofProviderError::InvalidProofResponse(format!(
                "Contract leaves length mismatch: expected {addresses_len} leaves for requested \
                 contracts, got {leaves_len}"
            )));
        }

        let mut proof_state = StateMaps::default();
        let commitment_infos = Self::build_commitment_infos(rpc_proof, query)?;

        // Update proof_state with class hashes and nonces from the proof.
        // We've validated the lengths match, so this zip is safe.
        for (leaf, addr) in
            rpc_proof.contracts_proof.contract_leaves_data.iter().zip(&query.contract_addresses)
        {
            proof_state.class_hashes.insert(*addr, ClassHash(leaf.class_hash));
            proof_state.nonces.insert(*addr, Nonce(leaf.nonce));
        }

        Ok(StorageProofs { proof_state, commitment_infos })
    }

    fn build_commitment_infos(
        rpc_proof: &RpcStorageProof,
        query: &RpcStorageProofsQuery,
    ) -> Result<StateCommitmentInfos, ProofProviderError> {
        let contracts_tree_root = HashOutput(rpc_proof.global_roots.contracts_tree_root);
        let classes_tree_root = HashOutput(rpc_proof.global_roots.classes_tree_root);

        let contracts_trie_commitment_info = CommitmentInfo {
            previous_root: contracts_tree_root,
            // The assumption is that the txs don`t change the state.
            updated_root: contracts_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&Self::rpc_nodes_to_preimage_map(
                &rpc_proof.contracts_proof.nodes,
            )),
        };

        let classes_trie_commitment_info = CommitmentInfo {
            // The assumption is that the txs don`t change the state.
            previous_root: classes_tree_root,
            updated_root: classes_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&Self::rpc_nodes_to_preimage_map(
                &rpc_proof.classes_proof,
            )),
        };

        let storage_tries_commitment_infos =
            Self::build_storage_commitment_infos(rpc_proof, query)?;

        Ok(StateCommitmentInfos {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        })
    }

    /// Builds commitment info for each contract's storage trie.
    ///
    /// This function processes all contracts in the query, creating storage commitment info
    /// for each. Contracts without storage reads will have empty `storage_keys` arrays and
    /// will receive proofs demonstrating empty storage.
    ///
    /// For each contract:
    /// 1. Validates that `contracts_storage_proofs` length matches `contract_storage_keys`
    /// 2. Looks up the contract's leaf data to get the storage root
    /// 3. Creates a `CommitmentInfo` with:
    ///    - `storage_root` from the contract leaf (or `Felt::ZERO` for empty storage)
    ///    - `commitment_facts` from the storage proof (may be empty for empty trees)
    ///
    /// # Empty Storage Trees
    ///
    /// Contracts with empty storage trees will have:
    /// - `storage_root`: `None` in the leaf data → converted to `Felt::ZERO`
    /// - `commitment_facts`: Empty HashMap (no nodes to traverse)
    ///
    /// This is valid - the OS verifies empty trees by checking the zero root with empty facts.
    fn build_storage_commitment_infos(
        rpc_proof: &RpcStorageProof,
        query: &RpcStorageProofsQuery,
    ) -> Result<HashMap<ContractAddress, CommitmentInfo>, ProofProviderError> {
        let storage_proofs = &rpc_proof.contracts_storage_proofs;

        // Validate that storage proofs match the number of contracts with storage keys.
        if storage_proofs.len() != query.contract_storage_keys.len() {
            return Err(ProofProviderError::InvalidProofResponse(format!(
                "Storage proofs length mismatch: expected {} proofs for contracts with storage \
                 keys, got {}",
                query.contract_storage_keys.len(),
                storage_proofs.len()
            )));
        }

        // Build a lookup map from contract address to leaf data.
        // This allows us to find the storage root for each contract with storage keys.
        let addr_to_leaf: HashMap<ContractAddress, _> = query
            .contract_addresses
            .iter()
            .zip(&rpc_proof.contracts_proof.contract_leaves_data)
            .map(|(addr, leaf)| (*addr, leaf))
            .collect();

        // Process each contract that has storage keys requested.
        query
            .contract_storage_keys
            .iter()
            .zip(storage_proofs)
            .map(|(contract_storage_keys, storage_proof)| {
                let addr = ContractAddress::try_from(contract_storage_keys.contract_address)
                    .map_err(|e| {
                        ProofProviderError::InvalidProofResponse(format!(
                            "Invalid contract address in storage keys: {e}"
                        ))
                    })?;

                let leaf = addr_to_leaf.get(&addr).ok_or_else(|| {
                    ProofProviderError::InvalidProofResponse(format!(
                        "Contract address {addr:?} in storage_keys not found in contract_addresses"
                    ))
                })?;

                // Handle empty storage tree: use zero root if storage_root is None.
                // Empty storage is valid - some contracts have no storage variables set.
                let storage_root = HashOutput(leaf.storage_root.unwrap_or(Felt::ZERO));

                Ok((
                    addr,
                    CommitmentInfo {
                        previous_root: storage_root,
                        updated_root: storage_root,
                        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                        commitment_facts: flatten_preimages(&Self::rpc_nodes_to_preimage_map(
                            storage_proof,
                        )),
                    },
                ))
            })
            .collect()
    }
}

#[async_trait]
impl StorageProofProvider for RpcStorageProofsProvider {
    async fn get_storage_proofs(
        &self,
        block_number: BlockNumber,
        execution_data: &VirtualBlockExecutionData,
    ) -> Result<StorageProofs, ProofProviderError> {
        let query = Self::prepare_query(execution_data);

        let rpc_proof = self.fetch_proofs(block_number, &query).await?;

        Self::to_storage_proofs(&rpc_proof, &query)
    }
}
