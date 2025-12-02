use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};

use crate::block_committer::input::{Config, ConfigImpl, StarknetStorageValue};
use crate::db::forest_trait::ForestReader;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub(crate) trait TrieReader {
    fn create_contracts_trie<'a>(
        &mut self,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)>;

    fn create_storage_tries<'a>(
        &mut self,
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        config: &impl Config,
        storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
    ) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>>;

    fn create_classes_trie<'a>(
        &mut self,
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        config: &impl Config,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<OriginalSkeletonTreeImpl<'a>>;
}

impl<'a, DB: TrieReader> ForestReader<'a> for DB {
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
