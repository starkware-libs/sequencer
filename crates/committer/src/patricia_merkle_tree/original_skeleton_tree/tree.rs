use std::collections::HashMap;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use crate::storage::storage_trait::Storage;

pub(crate) type OriginalSkeletonNodeMap = HashMap<NodeIndex, OriginalSkeletonNode>;
pub(crate) type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the unmodified
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait OriginalSkeletonTree<'a>: Sized {
    fn create<L: LeafData>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<Self>;

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap;

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap;

    fn create_and_get_previous_leaves<L: LeafData>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)>;

    #[allow(dead_code)]
    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a>;
}

// TODO(Dori, 1/7/2024): Make this a tuple struct.
#[derive(Debug, PartialEq)]
pub(crate) struct OriginalSkeletonTreeImpl<'a> {
    pub(crate) nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    pub(crate) sorted_leaf_indices: SortedLeafIndices<'a>,
}

impl<'a> OriginalSkeletonTree<'a> for OriginalSkeletonTreeImpl<'a> {
    fn create<L: LeafData>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<Self> {
        Self::create_impl(storage, root_hash, sorted_leaf_indices, config)
    }

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap {
        &mut self.nodes
    }

    fn create_and_get_previous_leaves<L: LeafData>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)> {
        Self::create_and_get_previous_leaves_impl(storage, root_hash, sorted_leaf_indices, config)
    }

    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a> {
        self.sorted_leaf_indices
    }
}
