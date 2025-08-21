use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_patricia::generate_trie_config;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::traversal::{fetch_patricia_paths, TraversalResult};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::storage_trait::Storage;

use crate::block_committer::input::{try_node_index_into_contract_address, StarknetStorageValue};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    CompiledClassHash,
    ContractsTrieProof,
    StarknetForestProofs,
};
generate_trie_config!(OriginalSkeletonStorageTrieConfig, StarknetStorageValue);

generate_trie_config!(OriginalSkeletonClassesTrieConfig, CompiledClassHash);

pub(crate) struct OriginalSkeletonContractsTrieConfig;

impl OriginalSkeletonTreeConfig<ContractState> for OriginalSkeletonContractsTrieConfig {
    fn compare_modified_leaves(&self) -> bool {
        false
    }
}

impl OriginalSkeletonContractsTrieConfig {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[allow(dead_code)]
/// Fetch all tries patricia paths given the modified leaves.
/// Fetch the leaves in the contracts trie only, to be able to get the storage root hashes.
/// Assumption: `contract_sorted_leaf_indices` contains all `contract_storage_sorted_leaf_indices`
/// keys.
fn fetch_all_patricia_paths(
    storage: &impl Storage,
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
    )?;

    // Contracts trie - the leaves are required.
    let mut leaves = HashMap::new();
    let contracts_proof_nodes = fetch_patricia_paths::<ContractState>(
        storage,
        contracts_trie_root_hash,
        contract_sorted_leaf_indices,
        Some(&mut leaves),
    )?;

    // Contracts storage tries.
    let mut contracts_trie_storage_proofs =
        HashMap::with_capacity(contract_storage_sorted_leaf_indices.len());

    for (idx, sorted_leaf_indices) in contract_storage_sorted_leaf_indices {
        let storage_root_hash = leaves
            .get(idx)
            .expect("Contract address must exist in the contracts trie leaves data.")
            .storage_root_hash;
        // No need to fetch the leaves.
        let leaves = None;
        let proof = fetch_patricia_paths::<StarknetStorageValue>(
            storage,
            storage_root_hash,
            *sorted_leaf_indices,
            leaves,
        )?;
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
