use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::HasStaticPrefix;
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

/// An enum that specifies how to treat unmodified children during the construction of the
/// [crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree]..
pub enum UnmodifiedChildTraversal {
    // Indicates that the child must be read since we don't have the data to create an original
    // tree node.
    Traverse,
    // Indicates that the child should be skipped as it's unmodified and we have its hash.
    Skip(HashOutput),
}

pub type TraversalResult<T> = Result<T, TraversalError>;

/// A trait that allows traversing a trie without knowledge of the concrete node types and data or
/// storage layout.
pub trait SubTreeTrait<'a>: Sized {
    /// Extra data a node can carry (e.g. its hash).
    type NodeData;

    /// Extra context needed to deserialize a node from a raw DbValue. For more info, see
    /// `DeserializeContext` in the `DBObject` trait.
    type NodeDeserializeContext;

    /// Creates a concrete child node given its index and data.
    fn create(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        node_data: Self::NodeData,
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
        bottom_data: Self::NodeData,
    ) -> (Self, Vec<&'a NodeIndex>) {
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
            .collect();

        (Self::create(bottom_leaves, bottom_index, bottom_data), previously_empty_leaf_indices)
    }

    fn get_children_subtrees(
        &self,
        left_data: Self::NodeData,
        right_data: Self::NodeData,
    ) -> (Self, Self) {
        let [left_leaves, right_leaves] = self.split_leaves();
        let left_root_index = self.get_root_index() * 2.into();
        (
            Self::create(left_leaves, left_root_index, left_data),
            Self::create(right_leaves, left_root_index + NodeIndex::ROOT, right_data),
        )
    }

    fn is_leaf(&self) -> bool {
        self.get_root_index().is_leaf()
    }

    /// Decide whether to traverse an unmodified child during the construction of the
    /// [crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree].
    fn should_traverse_unmodified_child(data: Self::NodeData) -> UnmodifiedChildTraversal;

    /// Returns the [DbKeyPrefix] of the root node.
    fn get_root_prefix<L: Leaf>(
        &self,
        key_context: &<L as HasStaticPrefix>::KeyContext,
    ) -> DbKeyPrefix;

    /// Returns the suffix of the root's db key.
    fn get_root_suffix(&self) -> Vec<u8>;

    /// Returns the `Self::NodeDeserializeContext` that's needed to deserialize the root node from a
    /// raw `DbValue`.
    fn get_root_context(&self) -> Self::NodeDeserializeContext;
}
