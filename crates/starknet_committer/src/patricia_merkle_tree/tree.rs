use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress};
use starknet_patricia::generate_trie_config;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::traversal::{fetch_patricia_paths, TraversalResult};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::Storage;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
    StarknetStorageKey,
    StarknetStorageValue,
};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    class_hash_into_node_index,
    CompiledClassHash,
    ContractsProof,
    StorageProofs,
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
/// Assumption: `contract_addresses` contain all `ContractAddress`es in `contract_storage_keys`.
fn fetch_all_patricia_paths(
    storage: &impl Storage,
    classes_trie_root_hash: HashOutput,
    contracts_trie_root_hash: HashOutput,
    class_hashes: &[ClassHash],
    contract_addresses: &[ContractAddress],
    contract_storage_keys: &HashMap<ContractAddress, Vec<StarknetStorageKey>>,
) -> TraversalResult<StorageProofs> {
    // Verify that all contract addresses in `contract_storage_keys` are included in
    // `contract_addresses`.
    for address in contract_storage_keys.keys() {
        assert!(contract_addresses.contains(address), "Missing contract address: {:?}", address);
    }

    // Classes trie.
    let classes_proof = {
        let mut node_indices: Vec<NodeIndex> =
            class_hashes.iter().map(class_hash_into_node_index).collect();
        fetch_patricia_paths::<CompiledClassHash>(
            storage,
            classes_trie_root_hash,
            &mut node_indices,
            None,
        )?
    };

    // Contracts trie.
    let mut node_indices: Vec<NodeIndex> =
        contract_addresses.iter().map(contract_address_into_node_index).collect();
    let mut contract_leaves_data = HashMap::new();
    let contracts_proof_nodes = fetch_patricia_paths::<ContractState>(
        storage,
        contracts_trie_root_hash,
        &mut node_indices,
        Some(&mut contract_leaves_data),
    )?;
    let contract_leaves_data: HashMap<ContractAddress, ContractState> = contract_leaves_data
        .into_iter()
        .map(|(idx, v)| {
            (
                try_node_index_into_contract_address(&idx)
                    .expect("Converting back NodeIndex to ContractAddress should succeed."),
                v,
            )
        })
        .collect();

    // Contracts storage tries.
    let mut contracts_storage_proofs = HashMap::with_capacity(contract_storage_keys.keys().len());

    for (address, keys) in contract_storage_keys {
        let storage_root_hash = contract_leaves_data
            .get(address)
            .expect("Contract address must exist in the contracts trie leaves data.")
            .storage_root_hash;
        if storage_root_hash.0 == Felt::ZERO {
            // No storage trie for this contract, it means we call this function with the previous
            // tree and the contract was added, or we call it with the new tree and the contract was
            // deleted.
            continue;
        }
        let mut node_indices = keys.iter().map(NodeIndex::from).collect::<Vec<NodeIndex>>();
        let proof = fetch_patricia_paths::<StarknetStorageValue>(
            storage,
            storage_root_hash,
            &mut node_indices,
            None,
        )?;
        contracts_storage_proofs.insert(*address, proof);
    }

    Ok(StorageProofs {
        classes_proof,
        contracts_proof: ContractsProof { nodes: contracts_proof_nodes, contract_leaves_data },
        contracts_storage_proofs,
    })
}

