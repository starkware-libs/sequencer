use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::Storage;

use crate::block_committer::input::{
    contract_address_into_node_index,
    Config,
    ConfigImpl,
    StarknetStorageValue,
};
use crate::db::forest_trait::{ForestReader, ForestWriter};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::{
    OriginalSkeletonClassesTrieConfig,
    OriginalSkeletonContractsTrieConfig,
    OriginalSkeletonStorageTrieConfig,
};
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct FactsDb<S: Storage> {
    // TODO(Yoav): Define StorageStats trait and impl it here. Then, make the storage field
    // private.
    pub storage: S,
}

impl<S: Storage> FactsDb<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Creates the contracts trie original skeleton.
    /// Also returns the previous contracts state of the modified contracts.
    fn create_contracts_trie<'a>(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)> {
        Ok(OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
            &mut self.storage,
            contracts_trie_root_hash,
            contracts_trie_sorted_indices,
            &HashMap::new(),
            &OriginalSkeletonContractsTrieConfig::new(),
        )?)
    }

    fn create_storage_tries<'a>(
        &mut self,
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
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
                &mut self.storage,
                contract_state.storage_root_hash,
                *sorted_leaf_indices,
                &config,
                updates,
            )?;
            storage_tries.insert(*address, original_skeleton);
        }
        Ok(storage_tries)
    }

    fn create_classes_trie<'a>(
        &mut self,
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        config: &impl Config,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<OriginalSkeletonTreeImpl<'a>> {
        let config = OriginalSkeletonClassesTrieConfig::new(config.warn_on_trivial_modifications());

        Ok(OriginalSkeletonTreeImpl::create(
            &mut self.storage,
            classes_trie_root_hash,
            contracts_trie_sorted_indices,
            &config,
            actual_classes_updates,
        )?)
    }
}

impl FactsDb<MapStorage> {
    pub fn consume_storage(self) -> MapStorage {
        self.storage
    }
}

impl<'a, S: Storage> ForestReader<'a> for FactsDb<S> {
    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    fn read(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ConfigImpl,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        let (contracts_trie, original_contracts_trie_leaves) = self.create_contracts_trie(
            contracts_trie_root_hash,
            forest_sorted_indices.contracts_trie_sorted_indices,
        )?;
        let storage_tries = self.create_storage_tries(
            storage_updates,
            &original_contracts_trie_leaves,
            &config,
            &forest_sorted_indices.storage_tries_sorted_indices,
        )?;
        let classes_trie = self.create_classes_trie(
            classes_updates,
            classes_trie_root_hash,
            &config,
            forest_sorted_indices.classes_trie_sorted_indices,
        )?;

        Ok((
            OriginalSkeletonForest { classes_trie, contracts_trie, storage_tries },
            original_contracts_trie_leaves,
        ))
    }
}

impl<S: Storage> ForestWriter for FactsDb<S> {
    fn write(&mut self, filled_forest: &FilledForest) -> usize {
        filled_forest.write_to_storage(&mut self.storage)
    }
}
