use std::collections::HashMap;

use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::create_tree_helper::TempSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

pub(crate) type UpdatedSkeletonNodeMap = HashMap<NodeIndex, UpdatedSkeletonNode>;
pub(crate) type UpdatedSkeletonTreeResult<T> = Result<T, UpdatedSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the unmodified nodes on the Merkle paths from the updated leaves
/// to the root.
pub trait UpdatedSkeletonTree<'a>: Sized + Send + Sync {
    /// Creates an updated tree from an original tree and modifications.
    fn create(
        original_skeleton: &mut impl OriginalSkeletonTree<'a>,
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
#[derive(Debug)]
pub struct UpdatedSkeletonTreeImpl {
    pub(crate) skeleton_tree: UpdatedSkeletonNodeMap,
}

impl<'a> UpdatedSkeletonTree<'a> for UpdatedSkeletonTreeImpl {
    fn create(
        original_skeleton: &mut impl OriginalSkeletonTree<'a>,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> UpdatedSkeletonTreeResult<Self> {
        if leaf_modifications.is_empty() {
            return Self::create_unmodified(original_skeleton);
        }
        let skeleton_tree = Self::finalize_bottom_layer(original_skeleton, leaf_modifications);

        let mut updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };

        let temp_root_node = updated_skeleton_tree.finalize_middle_layers(original_skeleton);
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
                        unreachable!(
                            "Root node cannot be unmodified when there are some modifications."
                        )
                    }
                };

                updated_skeleton_tree
                    .skeleton_tree
                    .insert(NodeIndex::ROOT, new_node)
                    .map_or((), |_| panic!("Root node already exists in the updated skeleton tree"))
            }
        };
        Ok(updated_skeleton_tree)
    }

    fn is_empty(&self) -> bool {
        // An updated skeleton tree is empty in two cases:
        // (i) The inner map is empty.
        // (ii)The root is considered as unmodified with a hash of an empty tree.
        let is_map_empty = self.skeleton_tree.is_empty();
        match self.skeleton_tree.get(&NodeIndex::ROOT) {
            Some(UpdatedSkeletonNode::UnmodifiedSubTree(root_hash)) => {
                *root_hash == HashOutput::ROOT_OF_EMPTY_TREE
            }
            Some(_modified_root) => false,
            None => {
                assert!(is_map_empty, "Non-empty tree must have a root node.");
                true
            }
        }
    }

    fn get_node(&self, index: NodeIndex) -> UpdatedSkeletonTreeResult<&UpdatedSkeletonNode> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode(index)),
        }
    }

    fn get_nodes(&self) -> impl Iterator<Item = (NodeIndex, UpdatedSkeletonNode)> {
        self.skeleton_tree.iter().map(|(index, node)| (*index, node.clone()))
    }
}
