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
    StarknetStorageProofs,
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
/// Assumption: `contract_sorted_leaf_indices` contains all `contract_storage_sorted_leaf_indices`
/// keys.
fn fetch_all_patricia_paths(
    storage: &impl Storage,
    classes_trie_root_hash: HashOutput,
    contracts_trie_root_hash: HashOutput,
    class_sorted_leaf_indices: SortedLeafIndices<'_>,
    contract_sorted_leaf_indices: SortedLeafIndices<'_>,
    contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
) -> TraversalResult<StarknetStorageProofs> {
    // Verify that all `contract_storage_sorted_leaf_indices` keys are included in
    // `contract_sorted_leaf_indices`.
    for address in contract_storage_sorted_leaf_indices.keys() {
        assert!(
            contract_sorted_leaf_indices.contains(address),
            "Missing contract address: {:?}",
            address
        );
    }

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
        HashMap::with_capacity(contract_storage_sorted_leaf_indices.keys().len());

    for (idx, sorted_leaf_indices) in contract_storage_sorted_leaf_indices {
        let storage_root_hash = leaves
            .get(idx)
            .expect("Contract address must exist in the contracts trie leaves data.")
            .storage_root_hash;
        if storage_root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            // No storage trie for this contract, it means we call this function with the previous
            // tree and the contract was added, or we call it with the new tree and the contract was
            // deleted.
            continue;
        }
        // No need to fetch the leaves.
        let leaves = None;
        let proof = fetch_patricia_paths::<StarknetStorageValue>(
            storage,
            storage_root_hash,
            *sorted_leaf_indices,
            leaves,
        )?;
        contracts_trie_storage_proofs.insert(
            try_node_index_into_contract_address(idx)
                .expect("Converting NodeIndex to ContractAddress should succeed."),
            proof,
        );
    }

    // Convert contract_leaves_data keys from NodeIndex to ContractAddress.
    let contract_leaves_data: HashMap<ContractAddress, ContractState> = leaves
        .into_iter()
        .map(|(idx, v)| {
            (
                try_node_index_into_contract_address(&idx)
                    .expect("Converting NodeIndex to ContractAddress should succeed."),
                v,
            )
        })
        .collect();

    Ok(StarknetStorageProofs {
        classes_trie_proof,
        contracts_trie_proof: ContractsTrieProof {
            nodes: contracts_proof_nodes,
            leaves: contract_leaves_data,
        },
        contracts_trie_storage_proofs,
    })
}
