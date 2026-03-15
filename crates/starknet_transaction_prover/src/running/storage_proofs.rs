use std::collections::HashMap;

use async_trait::async_trait;
use blockifier::state::cached_state::StateMaps;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_os::commitment_infos::{
    create_commitment_infos,
    CommitmentInfo,
    StateCommitmentInfos,
};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    flatten_preimages,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::{
    ConfirmedBlockId,
    ContractStorageKeys,
    ContractsProof,
    Felt,
    MerkleNode,
    StorageProof as RpcStorageProof,
};

use crate::errors::ProofProviderError;
use crate::running::committer_utils::{
    commit_state_diff,
    create_facts_db_from_storage_proof,
    state_maps_to_committer_state_diff,
    validate_virtual_os_state_diff,
};
use crate::running::virtual_block_executor::VirtualBlockExecutionData;

/// Pathfinder hard-codes `const MAX_KEYS: usize = 100` in `get_storage_proof`, counting
/// `class_hashes.len() + contract_addresses.len() + total_storage_keys`.
const MAX_KEYS_PER_REQUEST: usize = 100;

/// Counts total keys in a query, mirroring Pathfinder's counting logic.
pub(crate) fn count_total_keys(query: &RpcStorageProofsQuery) -> usize {
    query.class_hashes.len()
        + query.contract_addresses.len()
        + query.contract_storage_keys.iter().map(|csk| csk.storage_keys.len()).sum::<usize>()
}

/// A single "key" item from a flattened query — each variant counts as 1 toward the RPC limit.
pub(crate) enum QueryItem {
    ClassHash(Felt),
    ContractAddress(ContractAddress),
    StorageKey { contract_address: Felt, key: Felt },
}

/// Flattens a query into individual key items for chunking.
///
/// Contract storage entries with no keys are intentionally dropped — they contribute nothing to
/// the RPC key count and do not affect the storage proof result.
pub(crate) fn flatten_query(query: &RpcStorageProofsQuery) -> Vec<QueryItem> {
    let mut items = Vec::with_capacity(count_total_keys(query));
    items.extend(query.class_hashes.iter().copied().map(QueryItem::ClassHash));
    items.extend(query.contract_addresses.iter().copied().map(QueryItem::ContractAddress));
    for contract in &query.contract_storage_keys {
        for &key in &contract.storage_keys {
            items.push(QueryItem::StorageKey { contract_address: contract.contract_address, key });
        }
    }
    items
}

pub(crate) fn collect_query(items: &[QueryItem]) -> RpcStorageProofsQuery {
    let mut query = RpcStorageProofsQuery::default();
    for item in items {
        match item {
            QueryItem::ClassHash(h) => query.class_hashes.push(*h),
            QueryItem::ContractAddress(a) => query.contract_addresses.push(*a),
            QueryItem::StorageKey { contract_address, key } => {
                match query.contract_storage_keys.last_mut() {
                    Some(last) if last.contract_address == *contract_address => {
                        last.storage_keys.push(*key);
                    }
                    _ => {
                        query.contract_storage_keys.push(ContractStorageKeys {
                            contract_address: *contract_address,
                            storage_keys: vec![*key],
                        });
                    }
                }
            }
        }
    }
    query
}

/// Splits a query into sub-queries, each within the `max_keys` limit.
pub(crate) fn split_query(
    query: &RpcStorageProofsQuery,
    max_keys: usize,
) -> Vec<RpcStorageProofsQuery> {
    assert!(max_keys > 0, "max_keys must be positive");
    flatten_query(query).chunks(max_keys).map(collect_query).collect()
}

/// Merges multiple `RpcStorageProof` responses into one, preserving the original query's ordering.
///
/// Trie nodes (`classes_proof`, `contracts_proof.nodes`) are unioned across all responses — the
/// same hash always maps to the same node since all responses target the same block/trie.
///
/// Positional data (`contract_leaves_data`, `contracts_storage_proofs`) is reconstructed in the
/// original query's order. When a contract's storage keys were split across chunks, the merkle
/// nodes from all chunks are merged into a single entry.
pub(crate) fn merge_storage_proofs(
    proofs: Vec<RpcStorageProof>,
    split_queries: &[RpcStorageProofsQuery],
    original_query: &RpcStorageProofsQuery,
) -> RpcStorageProof {
    assert_eq!(proofs.len(), split_queries.len(), "proofs/queries length mismatch");
    assert!(!proofs.is_empty(), "cannot merge zero proofs");

    let global_roots = proofs[0].global_roots.clone();
    let mut classes_proof: IndexMap<Felt, MerkleNode> = IndexMap::default();
    let mut contracts_proof =
        ContractsProof { nodes: IndexMap::default(), contract_leaves_data: Vec::new() };

    let addr_to_idx: HashMap<Felt, usize> = original_query
        .contract_storage_keys
        .iter()
        .enumerate()
        .map(|(i, csk)| (csk.contract_address, i))
        .collect();
    let mut contracts_storage_proofs: Vec<IndexMap<Felt, MerkleNode>> =
        vec![IndexMap::default(); original_query.contract_storage_keys.len()];

    for (chunk_query, proof) in split_queries.iter().zip(proofs) {
        classes_proof.extend(proof.classes_proof);
        contracts_proof.nodes.extend(proof.contracts_proof.nodes);
        contracts_proof.contract_leaves_data.extend(proof.contracts_proof.contract_leaves_data);
        for (csk, storage_proof) in
            chunk_query.contract_storage_keys.iter().zip(proof.contracts_storage_proofs)
        {
            let idx = addr_to_idx
                .get(&csk.contract_address)
                .expect("chunk address not in original query");
            contracts_storage_proofs[*idx].extend(storage_proof);
        }
    }

    RpcStorageProof { classes_proof, contracts_proof, contracts_storage_proofs, global_roots }
}

/// Configuration for storage proof provider behavior.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StorageProofConfig {
    /// Whether to include state changes in the storage proofs.
    ///
    /// When `true`, the provider tracks state modifications and provides proofs for both the
    /// pre-execution and post-execution state roots, enabling verification of state
    /// transitions.
    ///
    /// When `false`, the provider only provides proofs for the initial state and assumes
    /// no state changes occur. This mode is suitable for read-only operations or when
    /// state verification is not required.
    pub(crate) include_state_changes: bool,
}

impl Default for StorageProofConfig {
    fn default() -> Self {
        Self { include_state_changes: true }
    }
}

/// Provides Patricia Merkle proofs for the initial state used in transaction execution.
///
/// This trait abstracts the retrieval of storage proofs, which are essential for OS input
/// generation. The proofs allow the OS to verify that the initial state values (read during
/// execution) are consistent with the global state commitment (Patricia root).
///
/// The returned `StorageProofs` contains:
/// - `contract_leaf_state`: Nonces and class hashes extracted from contract leaves.
/// - `commitment_infos`: The Patricia Merkle proof nodes for contracts, classes, and storage tries.
#[async_trait]
pub(crate) trait StorageProofProvider {
    async fn get_storage_proofs(
        &self,
        block_number: BlockNumber,
        execution_data: &VirtualBlockExecutionData,
        config: &StorageProofConfig,
    ) -> Result<StorageProofs, ProofProviderError>;
}

/// Query parameters for fetching storage proofs from RPC.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RpcStorageProofsQuery {
    pub(crate) class_hashes: Vec<Felt>,
    pub(crate) contract_addresses: Vec<ContractAddress>,
    pub(crate) contract_storage_keys: Vec<ContractStorageKeys>,
}

/// Complete OS input data built from RPC proofs.
pub(crate) struct StorageProofs {
    /// Extended initial reads with class hashes and nonces from the proof.
    /// Required by the OS to verify contract state.
    pub(crate) extended_initial_reads: StateMaps,
    /// Commitment infos for the extended initial reads.
    pub(crate) commitment_infos: StateCommitmentInfos,
}

/// Wrapper around `JsonRpcClient` for fetching storage proofs.
pub(crate) struct RpcStorageProofsProvider(pub(crate) JsonRpcClient<HttpTransport>);

impl RpcStorageProofsProvider {
    pub(crate) fn new(rpc_url: url::Url) -> Self {
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
    pub(crate) fn prepare_query(
        execution_data: &VirtualBlockExecutionData,
    ) -> RpcStorageProofsQuery {
        let class_hashes: Vec<Felt> =
            execution_data.executed_class_hashes.iter().map(|ch| ch.0).collect();

        let initial_reads = &execution_data.initial_reads;
        // Sort contract addresses for deterministic ordering (for offline replay mode).
        let mut contract_addresses: Vec<ContractAddress> =
            initial_reads.get_contract_addresses().into_iter().collect();
        contract_addresses.sort();

        // Group storage keys by address, then map over all contract_addresses (which may include
        // addresses with no storage reads) to build the output.
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

    /// Fetch storage proofs from RPC, automatically chunking if the total key count exceeds
    /// the node's per-request limit.
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
    pub(crate) async fn fetch_proofs(
        &self,
        block_number: BlockNumber,
        query: &RpcStorageProofsQuery,
    ) -> Result<RpcStorageProof, ProofProviderError> {
        if count_total_keys(query) <= MAX_KEYS_PER_REQUEST {
            return self.fetch_single_proof(block_number, query).await;
        }

        let chunks = split_query(query, MAX_KEYS_PER_REQUEST);
        // TODO(Aviv): Consider fetching chunks in parallel with try_join_all.
        let mut proofs = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            proofs.push(self.fetch_single_proof(block_number, chunk).await?);
        }

        Ok(merge_storage_proofs(proofs, &chunks, query))
    }

    /// Sends a single `get_storage_proof` RPC call (no chunking).
    async fn fetch_single_proof(
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

    /// Creates commitment infos from RPC storage proof and state changes.
    /// This function runs the committer to compute new state roots based on the execution data,
    /// then generates commitment infos using the facts stored in the committer's storage.
    pub(crate) async fn create_commitment_infos_with_state_changes(
        rpc_proof: &RpcStorageProof,
        query: &RpcStorageProofsQuery,
        extended_initial_reads: &StateMaps,
        state_diff: &StateMaps,
    ) -> Result<StateCommitmentInfos, ProofProviderError> {
        // Build FactsDb from RPC proofs and execution initial reads.
        let mut facts_db =
            create_facts_db_from_storage_proof(rpc_proof, query, extended_initial_reads)?;

        // Get initial state roots from RPC proof.
        let contracts_trie_root_hash = HashOutput(rpc_proof.global_roots.contracts_tree_root);
        let classes_trie_root_hash = HashOutput(rpc_proof.global_roots.classes_tree_root);
        // Convert the blockifier state maps to committer state diff and validate is stands with
        // the virtual OS assumptions.
        let committer_state_diff = state_maps_to_committer_state_diff(state_diff.clone());
        validate_virtual_os_state_diff(&committer_state_diff)?;

        // Commit state diff using the committer.
        let new_roots = commit_state_diff(
            &mut facts_db,
            contracts_trie_root_hash,
            classes_trie_root_hash,
            committer_state_diff,
        )
        .await?;

        let previous_state_roots = StateRoots { contracts_trie_root_hash, classes_trie_root_hash };

        // Consume the new facts from the committer storage.
        let mut map_storage: MapStorage = facts_db.consume_storage();

        // Get extended initial reads keys.
        let initial_reads_keys = extended_initial_reads.keys();

        // TODO(Aviv): Try to undertand if we can create classes trie commitment info
        // without the compiled class hashes.
        let mut commitment_infos = create_commitment_infos(
            &previous_state_roots,
            &new_roots,
            &mut map_storage,
            &initial_reads_keys,
        )
        .await
        .map_err(|e| ProofProviderError::BlockCommitmentError(e.to_string()))?;

        // The created commitment infos doesn't have the compiled class hashes,
        // as a result it doesn't have the classes trie commitment info.
        // We complement it with the RPC proof facts.
        let classes_rpc_facts =
            flatten_preimages(&Self::rpc_nodes_to_preimage_map(&rpc_proof.classes_proof));
        commitment_infos.classes_trie_commitment_info.commitment_facts.extend(classes_rpc_facts);

        Ok(commitment_infos)
    }

    /// Creates commitment infos from RPC storage proof without state changes.
    ///
    /// This function assumes that the new state roots equal the previous state roots.
    /// It sets `updated_root` equal to `previous_root` for all commitment infos (contracts,
    /// classes, and storage tries).
    fn create_commitment_infos_without_state_changes(
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
        config: &StorageProofConfig,
    ) -> Result<StorageProofs, ProofProviderError> {
        let query = Self::prepare_query(execution_data);

        let rpc_proof = self.fetch_proofs(block_number, &query).await?;

        // Validate that contract_leaves_data matches contract_addresses length.
        let leaves_len = rpc_proof.contracts_proof.contract_leaves_data.len();
        let addresses_len = query.contract_addresses.len();
        if leaves_len != addresses_len {
            return Err(ProofProviderError::InvalidProofResponse(format!(
                "Contract leaves length mismatch: expected {addresses_len} leaves for requested \
                 contracts, got {leaves_len}"
            )));
        }

        // Update initial reads with class hashes and nonces from the proof.
        // We've validated the lengths match, so this zip is safe.
        let mut extended_initial_reads = StateMaps::default();
        for (leaf, addr) in
            rpc_proof.contracts_proof.contract_leaves_data.iter().zip(&query.contract_addresses)
        {
            extended_initial_reads.class_hashes.insert(*addr, ClassHash(leaf.class_hash));
            extended_initial_reads.nonces.insert(*addr, Nonce(leaf.nonce));
        }

        // Include storage values from execution.
        extended_initial_reads.storage.extend(&execution_data.initial_reads.storage);

        let commitment_infos = match config.include_state_changes {
            true => {
                Self::create_commitment_infos_with_state_changes(
                    &rpc_proof,
                    &query,
                    &extended_initial_reads,
                    &execution_data.state_diff,
                )
                .await?
            }
            false => Self::create_commitment_infos_without_state_changes(&rpc_proof, &query)?,
        };

        Ok(StorageProofs { extended_initial_reads, commitment_infos })
    }
}
