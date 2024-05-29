use std::collections::HashMap;

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
    // A deleted node.
    Empty,
    // A new/modified leaf.
    Leaf,
    Original(OriginalSkeletonNode),
}

impl TempSkeletonNode {
    fn is_empty(&self) -> bool {
        *self == Self::Empty
    }
}

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
    /// Updates the originally empty Patricia-Merkle tree rooted at the given index, with leaf
    /// modifications (already updated in the skeleton mapping) in the given leaf_indices.
    /// Returns the root temporary skeleton node as inferred from the subtree.
    fn update_node_in_empty_tree(
        &mut self,
        root_index: &NodeIndex,
        leaf_indices: &[NodeIndex],
    ) -> TempSkeletonNode {
        if root_index.is_leaf() {
            // Leaf. As this is an empty tree, the leaf must be new.
            assert!(
                leaf_indices.len() == 1
                    && leaf_indices[0] == *root_index
                    && self.skeleton_tree.contains_key(root_index),
                "Unexpected leaf index (root_index={root_index:?}, leaf_indices={leaf_indices:?})."
            );
            return TempSkeletonNode::Leaf;
        }

        if has_leaves_on_both_sides(&self.tree_height, root_index, leaf_indices) {
            // Binary node.
            let [left_indices, right_indices] =
                split_leaves(&self.tree_height, root_index, leaf_indices);
            let [left_child_index, right_child_index] = root_index.get_children_indices();
            let left_child = self.update_node_in_empty_tree(&left_child_index, left_indices);
            let right_child = self.update_node_in_empty_tree(&right_child_index, right_indices);
            return self.node_from_binary_data(root_index, &left_child, &right_child);
        }

        // Edge node.
        let path_to_lca = get_path_to_lca(root_index, leaf_indices);
        let bottom_index = path_to_lca.bottom_index(*root_index);
        let bottom = self.update_node_in_empty_tree(&bottom_index, leaf_indices);
        self.node_from_edge_data(&path_to_lca, &bottom_index, &bottom)
    }

    #[allow(dead_code)]
    /// Updates the Patricia tree rooted at the given index, with the given leaves; returns the root.
    /// Assumes the given list of indices is sorted.
    fn update_node_in_nonempty_tree(
        &mut self,
        root_index: &NodeIndex,
        original_skeleton: &mut HashMap<NodeIndex, OriginalSkeletonNode>,
        leaf_indices: &[NodeIndex],
    ) -> TempSkeletonNode {
        if root_index.is_leaf() && leaf_indices.contains(root_index) {
            // A new/modified/deleted leaf.
            if self.skeleton_tree.contains_key(root_index) {
                // A new/modified leaf.
                return TempSkeletonNode::Leaf;
            } else {
                // A deleted leaf.
                return TempSkeletonNode::Empty;
            };
        };

        // Not a leaf or an unchanged leaf (a Sibling or unmodified bottom).
        let original_node = *original_skeleton
            .get(root_index)
            .unwrap_or_else(|| panic!("Node {root_index:?} not found."));

        if leaf_indices.is_empty() {
            match original_node {
                OriginalSkeletonNode::Binary => unreachable!(
                    "Index {root_index:?} is an original Binary node without leaf modifications - 
                    it should be a Sibling instead."
                ),
                OriginalSkeletonNode::UnmodifiedBottom(_) => unreachable!(
                    "Index {root_index:?} is an UnmodifiedBottom without leaf modifications. 
                    It shouldn't be reached as it must have an original Edge parent that would stop 
                    the recursion."
                ),
                OriginalSkeletonNode::Edge(_) | OriginalSkeletonNode::LeafOrBinarySibling(_) => {
                    return TempSkeletonNode::Original(original_node)
                }
            }
        };

        match original_node {
            OriginalSkeletonNode::LeafOrBinarySibling(_)
            | OriginalSkeletonNode::UnmodifiedBottom(_) => {
                unreachable!(
                    "A sibling/unmodified bottom can have no leaf_modifications in its subtree."
                )
            }
            OriginalSkeletonNode::Binary => {
                let [left_indices, right_indices] =
                    split_leaves(&self.tree_height, root_index, leaf_indices);
                let [left_child_index, right_child_index] = root_index.get_children_indices();
                let left = self.update_node_in_nonempty_tree(
                    &left_child_index,
                    original_skeleton,
                    left_indices,
                );
                let right = self.update_node_in_nonempty_tree(
                    &right_child_index,
                    original_skeleton,
                    right_indices,
                );
                self.node_from_binary_data(root_index, &left, &right)
            }
            OriginalSkeletonNode::Edge(path_to_bottom) => {
                self.update_edge_node(root_index, &path_to_bottom, original_skeleton, leaf_indices)
            }
        }
    }

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
                    match node {
                        TempSkeletonNode::Leaf => {
                            // Leaf is finalized in the initial phase of updated skeleton creation.
                            assert!(
                                self.skeleton_tree.contains_key(&index),
                                "Leaf index {index:?} doesn't appear in the skeleton."
                            );
                            continue;
                        }
                        TempSkeletonNode::Empty => unreachable!("Unexpected empty node."),
                        TempSkeletonNode::Original(_) => {
                            unreachable!("node is not an Original variant.")
                        }
                    }
                };
                let updated = match original_node {
                    OriginalSkeletonNode::Binary => UpdatedSkeletonNode::Binary,
                    OriginalSkeletonNode::Edge(path_to_bottom) => {
                        UpdatedSkeletonNode::Edge(*path_to_bottom)
                    }
                    OriginalSkeletonNode::LeafOrBinarySibling(_)
                    | OriginalSkeletonNode::UnmodifiedBottom(_) => {
                        // Unmodified nodes are finalized in the initial phase of updated skeleton
                        // creation.
                        continue;
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
            match bottom {
                TempSkeletonNode::Empty => {
                    return TempSkeletonNode::Empty;
                }
                TempSkeletonNode::Leaf => {
                    // Leaf is finalized in the initial phase of updated skeleton creation.
                    assert!(
                        self.skeleton_tree.contains_key(bottom_index),
                        "bottom is a non-empty leaf but doesn't appear in the skeleton."
                    );
                    return TempSkeletonNode::Original(OriginalSkeletonNode::Edge(*path));
                }
                TempSkeletonNode::Original(_) => unreachable!("bottom is not an Original variant."),
            };
        };
        TempSkeletonNode::Original(match original_node {
            OriginalSkeletonNode::Edge(path_to_bottom) => {
                OriginalSkeletonNode::Edge(path.concat_paths(*path_to_bottom))
            }
            OriginalSkeletonNode::Binary => {
                // Finalize bottom - a binary descendant cannot change form.
                self.skeleton_tree
                    .insert(*bottom_index, UpdatedSkeletonNode::Binary);
                OriginalSkeletonNode::Edge(*path)
            }
            OriginalSkeletonNode::LeafOrBinarySibling(_)
            | OriginalSkeletonNode::UnmodifiedBottom(_) => OriginalSkeletonNode::Edge(*path),
        })
    }

    /// Updates an original skeleton subtree rooted with an edge node.
    fn update_edge_node(
        &mut self,
        _root_index: &NodeIndex,
        _path_to_bottom: &PathToBottom,
        _original_skeleton: &mut HashMap<NodeIndex, OriginalSkeletonNode>,
        _leaf_indices: &[NodeIndex],
    ) -> TempSkeletonNode {
        todo!("Implement update_edge_node.")
    }
}
