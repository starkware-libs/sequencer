use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTree,
    OriginalSkeletonTreeImpl,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::storage_trait::Storage;

use crate::block_committer::input::{
    contract_address_into_node_index,
    Config,
    StarknetStorageValue,
};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::{
    OriginalSkeletonClassesTrieConfig,
    OriginalSkeletonContractsTrieConfig,
    OriginalSkeletonStorageTrieConfig,
};
use crate::patricia_merkle_tree::types::CompiledClassHash;

#[derive(Debug, PartialEq)]
pub(crate) struct OriginalSkeletonForest<'a> {
    pub(crate) classes_trie: OriginalSkeletonTreeImpl<'a>,
    pub(crate) contracts_trie: OriginalSkeletonTreeImpl<'a>,
    pub(crate) storage_tries: HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>,
}

impl<'a> OriginalSkeletonForest<'a> {
    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    pub(crate) fn create(
        storage: &impl Storage,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &ForestSortedIndices<'a>,
        config: &impl Config,
    ) -> ForestResult<(Self, HashMap<NodeIndex, ContractState>)>
    where
        Self: std::marker::Sized,
    {
        let (contracts_trie, original_contracts_trie_leaves) = Self::create_contracts_trie(
            contracts_trie_root_hash,
            storage,
            forest_sorted_indices.contracts_trie_sorted_indices,
        )?;
        let storage_tries = Self::create_storage_tries(
            storage_updates,
            &original_contracts_trie_leaves,
            storage,
            config,
            &forest_sorted_indices.storage_tries_sorted_indices,
        )?;
        let classes_trie = Self::create_classes_trie(
            classes_updates,
            classes_trie_root_hash,
            storage,
            config,
            forest_sorted_indices.classes_trie_sorted_indices,
        )?;

        Ok((Self { classes_trie, contracts_trie, storage_tries }, original_contracts_trie_leaves))
    }

    /// Creates the contracts trie original skeleton.
    /// Also returns the previous contracts state of the modified contracts.
    fn create_contracts_trie(
        contracts_trie_root_hash: HashOutput,
        storage: &impl Storage,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)> {
        Ok(OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
            storage,
            contracts_trie_root_hash,
            contracts_trie_sorted_indices,
            &OriginalSkeletonContractsTrieConfig::new(),
            &HashMap::new(),
        )?)
    }

    fn create_storage_tries(
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        storage: &impl Storage,
        config: &impl Config,
        storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
    ) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>> {
        let mut storage_tries = HashMap::new();
        for (address, updates) in actual_storage_updates {
            let sorted_leaf_indices = storage_tries_sorted_indices
                .get(address)
                .ok_or(ForestError::MissingSortedLeafIndices(*address))?;
            let contract_state = original_contracts_trie_leaves
                .get(&contract_address_into_node_index(address))
                .ok_or(ForestError::MissingContractCurrentState(*address))?;
            let config =
                OriginalSkeletonStorageTrieConfig::new(config.warn_on_trivial_modifications());

            let original_skeleton = OriginalSkeletonTreeImpl::create(
                storage,
                contract_state.storage_root_hash,
                *sorted_leaf_indices,
                &config,
                updates,
            )?;
            storage_tries.insert(*address, original_skeleton);
        }
        Ok(storage_tries)
    }

    fn create_classes_trie(
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        storage: &impl Storage,
        config: &impl Config,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<OriginalSkeletonTreeImpl<'a>> {
        let config = OriginalSkeletonClassesTrieConfig::new(config.warn_on_trivial_modifications());

        Ok(OriginalSkeletonTreeImpl::create(
            storage,
            classes_trie_root_hash,
            contracts_trie_sorted_indices,
            &config,
            actual_classes_updates,
        )?)
    }
}

/// Holds all the indices of the modified leaves in the Starknet forest grouped by tree and sorted.
pub(crate) struct ForestSortedIndices<'a> {
    pub(crate) storage_tries_sorted_indices: HashMap<ContractAddress, SortedLeafIndices<'a>>,
    pub(crate) contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    pub(crate) classes_trie_sorted_indices: SortedLeafIndices<'a>,
}
