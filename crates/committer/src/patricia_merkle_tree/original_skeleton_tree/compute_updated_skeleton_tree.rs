use std::collections::HashMap;

use ethnum::U256;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::LeafDataImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl, OriginalSkeletonTreeResult,
};
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[cfg(test)]
#[path = "compute_updated_skeleton_tree_test.rs"]
pub mod compute_updated_skeleton_tree_test;

impl OriginalSkeletonTreeImpl {
    pub(crate) fn compute_updated_skeleton_tree_impl(
        &self,
        _index_to_updated_leaf: HashMap<NodeIndex, LeafDataImpl>,
    ) -> OriginalSkeletonTreeResult<UpdatedSkeletonTreeImpl<LeafDataImpl>> {
        todo!()
    }

    fn get_node_height(&self, index: &NodeIndex) -> TreeHeight {
        TreeHeight::new(u8::from(self.tree_height) - index.bit_length() + 1)
    }

    #[allow(dead_code)]
    /// Returns the path from the given root_index to the LCA of the leaves. Assumes the leaves are:
    /// * Sorted.
    /// * Descendants of the given index.
    /// * Non-empty list.
    fn get_path_to_lca(&self, root_index: &NodeIndex, leaf_indices: &[NodeIndex]) -> PathToBottom {
        if leaf_indices.is_empty() {
            panic!("Unexpected empty array.");
        }
        let lca = if leaf_indices.len() == 1 {
            leaf_indices[0]
        } else {
            leaf_indices[0].get_lca(leaf_indices.last().expect("Unexpected empty array"))
        };
        root_index.get_path_to_descendant(lca)
    }

    /// Returns whether a root of a subtree has leaves on both sides. Assumes:
    /// * The leaf indices array is sorted.
    /// * All leaves are descendants of the root.
    #[allow(dead_code)]
    fn has_leaves_on_both_sides(&self, root_index: &NodeIndex, leaf_indices: &[NodeIndex]) -> bool {
        if leaf_indices.is_empty() {
            return false;
        }

        let root_height = self.get_node_height(root_index);
        let assert_child = |leaf_index: NodeIndex| {
            if (leaf_index >> root_height.into()) != *root_index {
                panic!("Leaf is not a descendant of the root.");
            }
        };

        let first_leaf = leaf_indices[0];
        assert_child(first_leaf);
        if leaf_indices.len() == 1 {
            return false;
        }

        let last_leaf = leaf_indices
            .last()
            .expect("leaf_indices unexpectedly empty.");
        assert_child(*last_leaf);

        let child_direction_mask = U256::ONE << (u8::from(root_height) - 1);
        (U256::from(first_leaf) & child_direction_mask)
            != (U256::from(*last_leaf) & child_direction_mask)
    }
}
