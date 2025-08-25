use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

#[cfg(test)]
#[path = "traversal_test.rs"]
pub mod traversal_test;

#[derive(Debug, PartialEq)]
pub(crate) struct SubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self) -> SubTreeHeight {
        SubTreeHeight::new(SubTreeHeight::ACTUAL_HEIGHT.0 - (self.root_index.bit_length() - 1))
    }

    pub(crate) fn split_leaves(&self) -> [SortedLeafIndices<'a>; 2] {
        split_leaves(&self.root_index, &self.sorted_leaf_indices)
    }

    pub(crate) fn is_unmodified(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    pub(crate) fn get_root_prefix<L: Leaf>(&self) -> PatriciaPrefix {
        if self.is_leaf() {
            PatriciaPrefix::Leaf(L::get_static_prefix())
        } else {
            PatriciaPrefix::InnerNode
        }
    }

    /// Returns the bottom subtree which is referred from `self` by the given path. When creating
    /// the bottom subtree some indices that were modified under `self` are not modified under the
    /// bottom subtree (leaves that were previously empty). These indices are returned as well.
    pub(crate) fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        bottom_hash: HashOutput,
    ) -> (Self, Vec<&NodeIndex>) {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let leftmost_index = self.sorted_leaf_indices.bisect_left(&leftmost_in_subtree);
        let rightmost_index = self.sorted_leaf_indices.bisect_right(&rightmost_in_subtree);
        let bottom_leaves = self.sorted_leaf_indices.subslice(leftmost_index, rightmost_index);
        let previously_empty_leaf_indices = self.sorted_leaf_indices.get_indices()
            [..leftmost_index]
            .iter()
            .chain(self.sorted_leaf_indices.get_indices()[rightmost_index..].iter())
            .collect();

        (
            Self {
                sorted_leaf_indices: bottom_leaves,
                root_index: bottom_index,
                root_hash: bottom_hash,
            },
            previously_empty_leaf_indices,
        )
    }

    pub(crate) fn get_children_subtrees(
        &self,
        left_hash: HashOutput,
        right_hash: HashOutput,
    ) -> (Self, Self) {
        let [left_leaves, right_leaves] = self.split_leaves();
        let left_root_index = self.root_index * 2.into();
        (
            SubTree {
                sorted_leaf_indices: left_leaves,
                root_index: left_root_index,
                root_hash: left_hash,
            },
            SubTree {
                sorted_leaf_indices: right_leaves,
                root_index: left_root_index + NodeIndex::ROOT,
                root_hash: right_hash,
            },
        )
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.root_index.is_leaf()
    }
}
