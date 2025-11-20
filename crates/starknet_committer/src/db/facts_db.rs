use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::Storage;

use crate::block_committer::input::{ConfigImpl, StarknetStorageValue};
use crate::db::forest_trait::{ForestReader, ForestWriter};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
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

    pub fn consume_storage(self) -> S {
        self.storage
    }
}

impl<'a, S: Storage> ForestReader<'a> for FactsDb<S> {
    fn read(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ConfigImpl,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        // TODO(Yoav): Move here OriginalSkeletonForest constructor  with all the (facts)
        // storage-related functions.
        OriginalSkeletonForest::create(
            &mut self.storage,
            contracts_trie_root_hash,
            classes_trie_root_hash,
            storage_updates,
            classes_updates,
            forest_sorted_indices,
            &config,
        )
    }
}

impl<S: Storage> ForestWriter for FactsDb<S> {
    fn write(&mut self, filled_forest: &FilledForest) -> usize {
        filled_forest.write_to_storage(&mut self.storage)
    }
}
