use std::collections::HashMap;

use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree: Sized + Send + Sync {
    /// Creates an updated tree from an original tree and modifications.
    #[allow(dead_code)]
    fn create(
        original_skeleton: impl OriginalSkeletonTree,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> Result<Self, UpdatedSkeletonTreeError>;

    /// Does the skeleton represents an empty-tree (i.e. all leaves are empty).
    #[allow(dead_code)]
    fn is_empty(&self) -> bool;

    /// Returns an iterator over all (node index, node) pairs in the tree.
    #[allow(dead_code)]
    fn get_nodes(&self) -> impl Iterator<Item = (NodeIndex, UpdatedSkeletonNode)>;

    /// Returns the node with the given index.
    #[allow(dead_code)]
    fn get_node(&self, index: NodeIndex) -> Result<&UpdatedSkeletonNode, UpdatedSkeletonTreeError>;
}

pub(crate) struct UpdatedSkeletonTreeImpl {
    pub(crate) skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode>,
}

impl UpdatedSkeletonTree for UpdatedSkeletonTreeImpl {
    fn create(
        _original_skeleton: impl OriginalSkeletonTree,
        _leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> Result<Self, UpdatedSkeletonTreeError> {
        todo!()
    }

    fn is_empty(&self) -> bool {
        todo!()
    }

    fn get_node(&self, index: NodeIndex) -> Result<&UpdatedSkeletonNode, UpdatedSkeletonTreeError> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode(index)),
        }
    }

    fn get_nodes(&self) -> impl Iterator<Item = (NodeIndex, UpdatedSkeletonNode)> {
        self.skeleton_tree
            .iter()
            .map(|(index, node)| (*index, node.clone()))
    }
}
