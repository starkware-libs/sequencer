use std::collections::HashMap;

use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::compute_updated_skeleton_tree::TempSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

pub(crate) type UpdatedSkeletonNodeMap = HashMap<NodeIndex, UpdatedSkeletonNode>;
pub(crate) type UpdatedSkeletonTreeResult<T> = Result<T, UpdatedSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree: Sized + Send + Sync {
    /// Creates an updated tree from an original tree and modifications.
    fn create(
        original_skeleton: &mut impl OriginalSkeletonTree,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> UpdatedSkeletonTreeResult<Self>;

    /// Does the skeleton represents an empty-tree (i.e. all leaves are empty).
    fn is_empty(&self) -> bool;

    /// Returns an iterator over all (node index, node) pairs in the tree.
    fn get_nodes(&self) -> impl Iterator<Item = (NodeIndex, UpdatedSkeletonNode)>;

    /// Returns the node with the given index.
    fn get_node(&self, index: NodeIndex) -> UpdatedSkeletonTreeResult<&UpdatedSkeletonNode>;
}
// TODO(Dori, 1/7/2024): Make this a tuple struct.
pub(crate) struct UpdatedSkeletonTreeImpl {
    pub(crate) skeleton_tree: UpdatedSkeletonNodeMap,
}

impl UpdatedSkeletonTree for UpdatedSkeletonTreeImpl {
    fn create(
        original_skeleton: &mut impl OriginalSkeletonTree,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> UpdatedSkeletonTreeResult<Self> {
        let skeleton_tree = Self::finalize_bottom_layer(original_skeleton, leaf_modifications);

        let mut updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };

        let temp_root_node =
            updated_skeleton_tree.finalize_middle_layers(original_skeleton, leaf_modifications);
        // Finalize root.
        match temp_root_node {
            TempSkeletonNode::Empty => assert!(updated_skeleton_tree.skeleton_tree.is_empty()),
            TempSkeletonNode::Leaf => {
                unreachable!("Root node cannot be a leaf")
            }
            TempSkeletonNode::Original(original_skeleton_node) => {
                let new_node = match original_skeleton_node {
                    OriginalSkeletonNode::Binary => UpdatedSkeletonNode::Binary,
                    OriginalSkeletonNode::Edge(path_to_bottom) => {
                        UpdatedSkeletonNode::Edge(path_to_bottom)
                    }
                    OriginalSkeletonNode::UnmodifiedSubTree(_) => {
                        unreachable!("Root node cannot be an unmodified.")
                    }
                };

                updated_skeleton_tree
                    .skeleton_tree
                    .insert(NodeIndex::ROOT, new_node)
                    .map_or((), |_| {
                        panic!("Root node already exists in the updated skeleton tree")
                    })
            }
        };
        Ok(updated_skeleton_tree)
    }

    fn is_empty(&self) -> bool {
        let is_empty = self.skeleton_tree.is_empty();
        if !is_empty {
            assert!(
                self.skeleton_tree.contains_key(&NodeIndex::ROOT),
                "Root node is missing from non-empty tree."
            );
        }
        is_empty
    }

    fn get_node(&self, index: NodeIndex) -> UpdatedSkeletonTreeResult<&UpdatedSkeletonNode> {
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
