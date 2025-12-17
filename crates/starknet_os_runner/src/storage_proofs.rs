use std::collections::HashMap;

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
pub trait StorageProofProvider {
    fn get_storage_proofs(
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

/// Converts RPC merkle nodes (hash → MerkleNode mapping) to a PreimageMap.
fn rpc_nodes_to_preimage_map<S: std::hash::BuildHasher>(
    nodes: &indexmap::IndexMap<Felt, starknet_rust_core::types::MerkleNode, S>,
) -> PreimageMap {
    nodes.iter().map(|(hash, node)| (HashOutput(*hash), Preimage::from(node))).collect()
}

/// Wrapper around `JsonRpcClient` for fetching storage proofs.
pub struct RpcStorageProofsProvider(pub JsonRpcClient<HttpTransport>);

impl RpcStorageProofsProvider {
    pub fn new(rpc_url: url::Url) -> Self {
        let transport = HttpTransport::new(rpc_url);
        let client = JsonRpcClient::new(transport);
        Self(client)
    }

    /// Extract query parameters from StateMaps.
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
    pub fn to_storage_proofs(
        rpc_proof: &RpcStorageProof,
        contract_addresses: &[ContractAddress],
    ) -> StorageProofs {
        let mut proof_state = StateMaps::default();
        let commitment_infos = Self::build_commitment_infos(rpc_proof, contract_addresses);

        // Update proof_state with class hashes and nonces from the proof.
        for (leaf, addr) in
            rpc_proof.contracts_proof.contract_leaves_data.iter().zip(contract_addresses)
        {
            proof_state.class_hashes.insert(*addr, ClassHash(leaf.class_hash));
            proof_state.nonces.insert(*addr, Nonce(leaf.nonce));
        }

        StorageProofs { proof_state, commitment_infos }
    }

    fn build_commitment_infos(
        rpc_proof: &RpcStorageProof,
        contract_addresses: &[ContractAddress],
    ) -> StateCommitmentInfos {
        let contracts_tree_root = HashOutput(rpc_proof.global_roots.contracts_tree_root);
        let classes_tree_root = HashOutput(rpc_proof.global_roots.classes_tree_root);

        let contracts_trie_commitment_info = CommitmentInfo {
            previous_root: contracts_tree_root,
            updated_root: contracts_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                &rpc_proof.contracts_proof.nodes,
            )),
        };

        let classes_trie_commitment_info = CommitmentInfo {
            previous_root: classes_tree_root,
            updated_root: classes_tree_root,
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                &rpc_proof.classes_proof,
            )),
        };

        let storage_tries_commitment_infos =
            Self::build_storage_commitment_infos(rpc_proof, contract_addresses);

        StateCommitmentInfos {
            contracts_trie_commitment_info,
            classes_trie_commitment_info,
            storage_tries_commitment_infos,
        }
    }

    /// Builds commitment info for each contract's storage trie.
    ///
    /// Each contract has its own storage trie with a separate root. This function:
    /// 1. Zips together: contract leaf data, addresses, and storage proofs (all in matching order)
    /// 2. For each contract that has a storage_root (via `filter_map`):
    ///    - Extracts the storage_root from the leaf data
    ///    - Converts the RPC storage proof nodes to commitment facts
    ///    - Creates a CommitmentInfo for that contract's storage trie
    ///
    /// The `filter_map` skips contracts where `storage_root` is `None` (contracts with no storage).
    fn build_storage_commitment_infos(
        rpc_proof: &RpcStorageProof,
        contract_addresses: &[ContractAddress],
    ) -> HashMap<ContractAddress, CommitmentInfo> {
        rpc_proof
            .contracts_proof
            .contract_leaves_data
            .iter()
            .zip(contract_addresses)
            .zip(&rpc_proof.contracts_storage_proofs)
            .filter_map(|((leaf, addr), storage_proof)| {
                // `?` returns None if storage_root is None, causing filter_map to skip this entry.
                let storage_root = HashOutput(leaf.storage_root?);
                Some((
                    *addr,
                    CommitmentInfo {
                        previous_root: storage_root,
                        updated_root: storage_root,
                        tree_height: SubTreeHeight::ACTUAL_HEIGHT,
                        commitment_facts: flatten_preimages(&rpc_nodes_to_preimage_map(
                            storage_proof,
                        )),
                    },
                ))
            })
            .collect()
    }
}

impl StorageProofProvider for RpcStorageProofsProvider {
    fn get_storage_proofs(
        &self,
        block_number: BlockNumber,
        execution_data: &VirtualBlockExecutionData,
    ) -> Result<StorageProofs, ProofProviderError> {
        let query = Self::prepare_query(execution_data);
        let contract_addresses = query.contract_addresses.clone();

        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let rpc_proof = runtime.block_on(self.fetch_proofs(block_number, &query))?;

        Ok(Self::to_storage_proofs(&rpc_proof, &contract_addresses))
    }
}
