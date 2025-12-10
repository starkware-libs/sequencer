use starknet_patricia_storage::errors::{DeserializationError, StorageError};
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, PatriciaStorageError};
use thiserror::Error;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

#[cfg(test)]
#[path = "traversal_test.rs"]
pub mod traversal_test;

#[derive(Debug, Error)]
pub enum TraversalError {
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    StorageRead(#[from] StorageError),
    #[error(transparent)]
    PatriciaStorage(#[from] PatriciaStorageError),
}

pub type TraversalResult<T> = Result<T, TraversalError>;

// The SubTreeTrait allows traversing a trie without knowledge of the concrete node types and data.
pub trait SubTreeTrait<'a>: Sized {
    // A node can carry data about its children (e.g. their hashes).
    type ChildData: Copy;

    // Creates a concrete child node given its index and data.
    fn create_child(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        child_data: Self::ChildData,
    ) -> Self;

    fn get_root_index(&self) -> NodeIndex;

    fn get_sorted_leaf_indices(&self) -> &SortedLeafIndices<'a>;

    fn get_height(&self) -> SubTreeHeight {
        SubTreeHeight::new(
            SubTreeHeight::ACTUAL_HEIGHT.0 - (self.get_root_index().bit_length() - 1),
        )
    }

    fn split_leaves(&self) -> [SortedLeafIndices<'a>; 2] {
        split_leaves(&self.get_root_index(), self.get_sorted_leaf_indices())
    }

    fn is_unmodified(&self) -> bool {
        self.get_sorted_leaf_indices().is_empty()
    }

    /// Returns the bottom subtree which is referred from `self` by the given path. When creating
    /// the bottom subtree some indices that were modified under `self` are not modified under the
    /// bottom subtree (leaves that were previously empty). These indices are returned as well.
    fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        bottom_data: Self::ChildData,
    ) -> (Self, Vec<NodeIndex>) {
        let sorted_leaf_indices = self.get_sorted_leaf_indices();
        let bottom_index = path_to_bottom.bottom_index(self.get_root_index());
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let leftmost_index = sorted_leaf_indices.bisect_left(&leftmost_in_subtree);
        let rightmost_index = sorted_leaf_indices.bisect_right(&rightmost_in_subtree);
        let bottom_leaves = sorted_leaf_indices.subslice(leftmost_index, rightmost_index);
        let previously_empty_leaf_indices = sorted_leaf_indices.get_indices()[..leftmost_index]
            .iter()
            .chain(sorted_leaf_indices.get_indices()[rightmost_index..].iter())
            .cloned()
            .collect();

        (
            Self::create_child(bottom_leaves, bottom_index, bottom_data),
            previously_empty_leaf_indices,
        )
    }

    fn get_children_subtrees(
        &self,
        left_data: Self::ChildData,
        right_data: Self::ChildData,
    ) -> (Self, Self) {
        let [left_leaves, right_leaves] = self.split_leaves();
        let left_root_index = self.get_root_index() * 2.into();
        (
            Self::create_child(left_leaves, left_root_index, left_data),
            Self::create_child(right_leaves, left_root_index + NodeIndex::ROOT, right_data),
        )
    }

    fn is_leaf(&self) -> bool {
        self.get_root_index().is_leaf()
    }

    // Indicates whether unmodified children should be traversed during the construction of the
    // skeleton tree.
    fn should_traverse_unmodified_children() -> bool;

    // Returns the db key prefix of the root node.
    fn get_root_prefix<L: Leaf>(&self) -> DbKeyPrefix;
}
