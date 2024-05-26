use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::types::TreeHeight;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[cfg(test)]
#[path = "compute_updated_skeleton_tree_test.rs"]
pub mod compute_updated_skeleton_tree_test;

#[derive(Debug, PartialEq, Eq)]
/// A temporary skeleton node used during the computation of the updated skeleton tree.
enum TempSkeletonNode {
    Empty,
    #[allow(dead_code)]
    Original(OriginalSkeletonNode),
}

impl TempSkeletonNode {
    fn is_empty(&self) -> bool {
        *self == Self::Empty
    }
}

#[allow(dead_code)]
/// Returns the path from the given root_index to the LCA of the leaves. Assumes the leaves are:
/// * Sorted.
/// * Descendants of the given index.
/// * Non-empty list.
fn get_path_to_lca(root_index: &NodeIndex, leaf_indices: &[NodeIndex]) -> PathToBottom {
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
fn has_leaves_on_both_sides(
    tree_height: &TreeHeight,
    root_index: &NodeIndex,
    leaf_indices: &[NodeIndex],
) -> bool {
    if leaf_indices.is_empty() {
        return false;
    }
    split_leaves(tree_height, root_index, leaf_indices)
        .iter()
        .all(|leaves_in_side| !leaves_in_side.is_empty())
}

impl UpdatedSkeletonTreeImpl {
    #[allow(dead_code)]
    /// Builds a (probably binary) node from its two updated children. Returns the TempSkeletonNode
    /// matching the given root for the subtree it is the root of. If one or more children are
    /// empty, the resulting node will not be binary.
    fn node_from_binary_data(
        &mut self,
        root_index: &NodeIndex,
        left: &TempSkeletonNode,
        right: &TempSkeletonNode,
    ) -> TempSkeletonNode {
        let [left_index, right_index] = root_index.get_children_indices();

        if !left.is_empty() && !right.is_empty() {
            // Both children are non-empty - a binary node.
            // Finalize children, as a binary node cannot change form.
            for (index, node) in [(left_index, left), (right_index, right)] {
                let TempSkeletonNode::Original(original_node) = node else {
                    unreachable!("Unexpected empty node.");
                };
                let updated = match original_node {
                    // Leaf is finalized upon updated skeleton creation.
                    OriginalSkeletonNode::Leaf(_) => continue,
                    OriginalSkeletonNode::Binary => UpdatedSkeletonNode::Binary,
                    OriginalSkeletonNode::Edge { path_to_bottom } => UpdatedSkeletonNode::Edge {
                        path_to_bottom: *path_to_bottom,
                    },
                    OriginalSkeletonNode::LeafOrBinarySibling(hash) => {
                        UpdatedSkeletonNode::Sibling(*hash)
                    }
                    OriginalSkeletonNode::UnmodifiedBottom(hash) => {
                        // TODO(Tzahi, 1/6/2024): create a new variant in UpdatedSkeletonNode.
                        UpdatedSkeletonNode::Sibling(*hash)
                    }
                };
                self.skeleton_tree.insert(index, updated);
            }

            return TempSkeletonNode::Original(OriginalSkeletonNode::Binary);
        }

        // At least one of the children is empty.
        let (child_node, child_index, child_direction) = if *right == TempSkeletonNode::Empty {
            (left, left_index, PathToBottom::LEFT_CHILD)
        } else {
            (right, right_index, PathToBottom::RIGHT_CHILD)
        };
        self.node_from_edge_data(&child_direction, &child_index, child_node)
    }

    /// Builds a (probably edge) node from its given updated descendant. Returns the
    /// TempSkeletonNode matching the given root (the source for the path to bottom) for the subtree
    /// it is the root of. If bottom is empty, returns an empty node.
    fn node_from_edge_data(
        &mut self,
        path: &PathToBottom,
        bottom_index: &NodeIndex,
        bottom: &TempSkeletonNode,
    ) -> TempSkeletonNode {
        let TempSkeletonNode::Original(original_node) = bottom else {
            return TempSkeletonNode::Empty;
        };
        TempSkeletonNode::Original(match original_node {
            OriginalSkeletonNode::Leaf(_) => {
                // Leaf is finalized upon updated skeleton creation.
                // bottom_index is in the updated skeleton iff it wasn't deleted from the tree.
                assert!(
                    self.skeleton_tree.contains_key(bottom_index),
                    "bottom is a non-empty leaf but doesn't appear in the skeleton."
                );
                OriginalSkeletonNode::Edge {
                    path_to_bottom: *path,
                }
            }
            OriginalSkeletonNode::Edge { path_to_bottom } => OriginalSkeletonNode::Edge {
                path_to_bottom: path.concat_paths(*path_to_bottom),
            },
            OriginalSkeletonNode::Binary => {
                // Finalize bottom - a binary descendant cannot change form.
                self.skeleton_tree
                    .insert(*bottom_index, UpdatedSkeletonNode::Binary);
                OriginalSkeletonNode::Edge {
                    path_to_bottom: *path,
                }
            }
            OriginalSkeletonNode::LeafOrBinarySibling(_)
            | OriginalSkeletonNode::UnmodifiedBottom(_) => OriginalSkeletonNode::Edge {
                path_to_bottom: *path,
            },
        })
    }
}
