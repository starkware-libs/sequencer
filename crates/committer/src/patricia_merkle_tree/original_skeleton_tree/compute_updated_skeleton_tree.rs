use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl, OriginalSkeletonTreeResult,
};
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[cfg(test)]
#[path = "compute_updated_skeleton_tree_test.rs"]
pub mod compute_updated_skeleton_tree_test;

impl OriginalSkeletonTreeImpl {
    pub(crate) fn compute_updated_skeleton_tree_impl(
        &self,
        _leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> OriginalSkeletonTreeResult<UpdatedSkeletonTreeImpl> {
        todo!()
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
        split_leaves(&self.tree_height, root_index, leaf_indices)
            .iter()
            .all(|leaves_in_side| !leaves_in_side.is_empty())
    }
}
