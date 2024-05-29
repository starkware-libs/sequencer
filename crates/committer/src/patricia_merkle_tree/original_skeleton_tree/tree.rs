use std::collections::HashMap;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};

use crate::storage::storage_trait::Storage;

#[allow(dead_code)]
pub(crate) type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the Sibling
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait OriginalSkeletonTree {
    #[allow(dead_code)]
    fn create(
        storage: &impl Storage,
        leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<Self>
    where
        Self: std::marker::Sized;

    fn get_nodes(&self) -> &HashMap<NodeIndex, OriginalSkeletonNode>;
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct OriginalSkeletonTreeImpl {
    pub(crate) nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    pub(crate) tree_height: TreeHeight,
}

impl OriginalSkeletonTree for OriginalSkeletonTreeImpl {
    fn create(
        storage: &impl Storage,
        sorted_leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<Self> {
        Self::create_impl(storage, sorted_leaf_indices, root_hash, tree_height)
    }

    fn get_nodes(&self) -> &HashMap<NodeIndex, OriginalSkeletonNode> {
        &self.nodes
    }
}
