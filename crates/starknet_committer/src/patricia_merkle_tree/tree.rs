use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::traversal::TraversalResult;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::storage_trait::Storage;

use crate::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
    StarknetStorageKey,
    StarknetStorageValue,
};
use crate::db::facts_db::traversal::fetch_patricia_paths;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    class_hash_into_node_index,
    CompiledClassHash,
    ContractsTrieProof,
    RootHashes,
    StarknetForestProofs,
};

#[derive(Default)]
pub struct OriginalSkeletonTrieConfig {
    compare_modified_leaves: bool,
}

impl OriginalSkeletonTrieConfig {
    pub fn new_for_contracts_trie() -> Self {
        Self { compare_modified_leaves: false }
    }

    pub fn new_for_classes_or_storage_trie(warn_on_trivial_modifications: bool) -> Self {
        Self { compare_modified_leaves: warn_on_trivial_modifications }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(should_compare_modified_leaves: bool) -> Self {
        Self { compare_modified_leaves: should_compare_modified_leaves }
    }
}

impl OriginalSkeletonTreeConfig for OriginalSkeletonTrieConfig {
    fn compare_modified_leaves(&self) -> bool {
        self.compare_modified_leaves
    }
}

/// Fetch all tries patricia paths given the modified leaves.
/// Fetch the leaves in the contracts trie only, to be able to get the storage root hashes.
/// Assumption: `contract_sorted_leaf_indices` contains all `contract_storage_sorted_leaf_indices`
/// keys.
async fn fetch_all_patricia_paths(
    storage: &mut impl Storage,
    classes_trie_root_hash: HashOutput,
    contracts_trie_root_hash: HashOutput,
    class_sorted_leaf_indices: SortedLeafIndices<'_>,
    contract_sorted_leaf_indices: SortedLeafIndices<'_>,
    contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
) -> TraversalResult<StarknetForestProofs> {
    // Verify that all `contract_storage_sorted_leaf_indices` keys are included in
    // `contract_sorted_leaf_indices`.
    let mut address_counter = 0;
    for address in contract_sorted_leaf_indices.get_indices().iter() {
        if contract_storage_sorted_leaf_indices.contains_key(address) {
            address_counter += 1;
        }
    }
    assert_eq!(
        address_counter,
        contract_storage_sorted_leaf_indices.len(),
        "contract_sorted_leaf_indices is missing an address with requested storage witnesses. \
         contract_sorted_leaf_indices: {contract_sorted_leaf_indices:?}, storage addresses: {:?}",
        contract_storage_sorted_leaf_indices.keys()
    );

    // Classes trie - no need to fetch the leaves.
    let leaves = None;
    let classes_trie_proof = fetch_patricia_paths::<CompiledClassHash>(
        storage,
        classes_trie_root_hash,
        class_sorted_leaf_indices,
        leaves,
        &EmptyKeyContext,
    )
    .await?;

    // Contracts trie - the leaves are required.
    let mut leaves = HashMap::new();
    let contracts_proof_nodes = fetch_patricia_paths::<ContractState>(
        storage,
        contracts_trie_root_hash,
        contract_sorted_leaf_indices,
        Some(&mut leaves),
        &EmptyKeyContext,
    )
    .await?;

    // Contracts storage tries.
    let mut contracts_trie_storage_proofs =
        HashMap::with_capacity(contract_storage_sorted_leaf_indices.len());

    for (idx, sorted_leaf_indices) in contract_storage_sorted_leaf_indices {
        // The contract address might not exist in the contracts trie in the following cases:
        // 1. We are looking at the previous tree and the contract is new.
        // 2. We are looking at the new tree and the contract is deleted (revert).
        // In either case, the storage trie of this contract is empty, so there is nothing to
        // prove regarding the contract storage.
        let Some(storage_root_hash) = leaves.get(idx).map(|leaf| leaf.storage_root_hash) else {
            continue;
        };
        // No need to fetch the leaves.
        let leaves = None;
        let proof = fetch_patricia_paths::<StarknetStorageValue>(
            storage,
            storage_root_hash,
            *sorted_leaf_indices,
            leaves,
            &EmptyKeyContext,
        )
        .await?;
        contracts_trie_storage_proofs.insert(
            try_node_index_into_contract_address(idx).unwrap_or_else(|_| {
                panic!(
                    "Converting leaf NodeIndex to ContractAddress should succeed; failed to \
                     convert {idx:?}."
                )
            }),
            proof,
        );
    }

    // Convert contract_leaves_data keys from NodeIndex to ContractAddress.
    let contract_leaves_data: HashMap<ContractAddress, ContractState> = leaves
        .into_iter()
        .map(|(idx, v)| {
            (
                try_node_index_into_contract_address(&idx).unwrap_or_else(|_| {
                    panic!(
                        "Converting leaf NodeIndex to ContractAddress should succeed; failed to \
                         convert {idx:?}."
                    )
                }),
                v,
            )
        })
        .collect();

    Ok(StarknetForestProofs {
        classes_trie_proof,
        contracts_trie_proof: ContractsTrieProof {
            nodes: contracts_proof_nodes,
            leaves: contract_leaves_data,
        },
        contracts_trie_storage_proofs,
    })
}

/// Fetch the Patricia paths (inner nodes) in the classes trie, contracts trie,
/// and contracts storage tries for both the previous and new root hashes.
/// Fetch the leaves in the contracts trie only, to be able to get the storage root hashes.
pub async fn fetch_previous_and_new_patricia_paths(
    storage: &mut impl Storage,
    classes_trie_root_hashes: RootHashes,
    contracts_trie_root_hashes: RootHashes,
    class_hashes: &[ClassHash],
    contract_addresses: &[ContractAddress],
    contract_storage_keys: &HashMap<ContractAddress, Vec<StarknetStorageKey>>,
) -> TraversalResult<StarknetForestProofs> {
    let mut class_leaf_indices: Vec<NodeIndex> =
        class_hashes.iter().map(class_hash_into_node_index).collect();
    let class_sorted_leaf_indices = SortedLeafIndices::new(&mut class_leaf_indices);

    let mut contract_leaf_indices: Vec<NodeIndex> =
        contract_addresses.iter().map(contract_address_into_node_index).collect();
    let contract_sorted_leaf_indices = SortedLeafIndices::new(&mut contract_leaf_indices);

    let mut contract_storage_leaf_indices: HashMap<NodeIndex, Vec<NodeIndex>> =
        contract_storage_keys
            .iter()
            .map(|(address, keys)| {
                let node_index = contract_address_into_node_index(address);
                let leaf_indices: Vec<_> = keys.iter().map(NodeIndex::from).collect();
                (node_index, leaf_indices)
            })
            .collect();
    let contract_storage_sorted_leaf_indices = &contract_storage_leaf_indices
        .iter_mut()
        .map(|(address, leaf_indices)| (*address, SortedLeafIndices::new(leaf_indices)))
        .collect();

    let prev_proofs = fetch_all_patricia_paths(
        storage,
        classes_trie_root_hashes.previous_root_hash,
        contracts_trie_root_hashes.previous_root_hash,
        class_sorted_leaf_indices,
        contract_sorted_leaf_indices,
        contract_storage_sorted_leaf_indices,
    )
    .await?;
    let new_proofs = fetch_all_patricia_paths(
        storage,
        classes_trie_root_hashes.new_root_hash,
        contracts_trie_root_hashes.new_root_hash,
        class_sorted_leaf_indices,
        contract_sorted_leaf_indices,
        contract_storage_sorted_leaf_indices,
    )
    .await?;

    let mut proofs = prev_proofs;
    proofs.extend(new_proofs);

    Ok(proofs)
}
