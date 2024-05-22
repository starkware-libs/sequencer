use crate::patricia_merkle_tree::node_data::inner_node::EdgeData;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::LeafModifications;
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[cfg(test)]
#[path = "compute_updated_skeleton_tree_test.rs"]
pub mod compute_updated_skeleton_tree_test;

#[derive(Debug, PartialEq, Eq)]
/// A temporary skeleton node used to during the computation of the updated skeleton tree.
enum TempSkeletonNode {
    Empty,
    #[allow(dead_code)]
    Original(OriginalSkeletonNode),
}

impl OriginalSkeletonTreeImpl {
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

impl UpdatedSkeletonTreeImpl {
    #[allow(dead_code)]
    /// Builds a (probably edge) node from its given descendant. Returns the TempSkeletonNode
    /// matching the given root (the source for the path to bottom) for the subtree it is the root
    /// of. If bottom is empty, returns an empty node.
    fn node_from_edge_data(
        &mut self,
        path: &PathToBottom,
        bottom_index: &NodeIndex,
        bottom: &TempSkeletonNode,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> TempSkeletonNode {
        TempSkeletonNode::Original(match bottom {
            TempSkeletonNode::Empty => return TempSkeletonNode::Empty,
            TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(_)) => {
                let leaf = leaf_modifications
                    .get(bottom_index)
                    .unwrap_or_else(|| panic!("Leaf modification {bottom_index:?} not found"));
                match leaf {
                    SkeletonLeaf::Zero => {
                        return TempSkeletonNode::Empty;
                    }
                    SkeletonLeaf::NonZero => {
                        // TODO(Tzahi, 1/6/2024): Consider inserting all modification
                        //leaves at one go and remove this.
                        // Finalize bottom leaf node (may happen multiple times)
                        self.skeleton_tree
                            .insert(*bottom_index, UpdatedSkeletonNode::Leaf(*leaf));
                        OriginalSkeletonNode::Edge {
                            path_to_bottom: *path,
                        }
                    }
                }
            }
            TempSkeletonNode::Original(OriginalSkeletonNode::Edge { path_to_bottom }) => {
                OriginalSkeletonNode::Edge {
                    path_to_bottom: path.concat_paths(*path_to_bottom),
                }
            }
            TempSkeletonNode::Original(OriginalSkeletonNode::EdgeSibling(edge_data)) => {
                OriginalSkeletonNode::EdgeSibling(EdgeData {
                    bottom_hash: edge_data.bottom_hash,
                    path_to_bottom: path.concat_paths(edge_data.path_to_bottom),
                })
            }
            TempSkeletonNode::Original(OriginalSkeletonNode::Binary) => {
                // Finalize bottom - a binary descendant cannot change form.
                self.skeleton_tree
                    .insert(*bottom_index, UpdatedSkeletonNode::Binary);
                OriginalSkeletonNode::Edge {
                    path_to_bottom: *path,
                }
            }
            TempSkeletonNode::Original(OriginalSkeletonNode::LeafOrBinarySibling(_)) => {
                OriginalSkeletonNode::Edge {
                    path_to_bottom: *path,
                }
            }
        })
    }
}
