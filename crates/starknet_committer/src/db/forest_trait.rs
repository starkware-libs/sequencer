use std::collections::HashMap;
use std::future::Future;

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;

use crate::block_committer::input::{ConfigImpl, StarknetStorageValue};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Trait for reading an original skeleton forest from some storage.
/// The implementation may depend on the underlying storage layout.
pub trait ForestReader<'a> {
    fn read(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        // TODO(Yoav): Change to 'impl Config' or delete this trait
        config: ConfigImpl,
    ) -> impl Future<
        Output = ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>,
    > + Send;
}

pub trait ForestWriter {
    /// Returns the number of new facts written to storage.
    fn write(&mut self, filled_forest: &FilledForest) -> impl Future<Output = usize> + Send;
}

pub trait ForestStorage<'a>: ForestReader<'a> + ForestWriter {}
