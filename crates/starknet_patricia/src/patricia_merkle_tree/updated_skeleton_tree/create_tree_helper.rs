use std::collections::HashMap;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonNodeMap,
    OriginalSkeletonTree,
};
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonNodeMap,
    UpdatedSkeletonTreeImpl,
    UpdatedSkeletonTreeResult,
};

#[cfg(test)]
#[path = "create_tree_helper_test.rs"]
pub mod create_tree_helper_test;

#[derive(Debug, PartialEq, Eq)]
/// A temporary skeleton node used during the computation of the updated skeleton tree.
pub(crate) enum TempSkeletonNode {
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

/// Returns the path from the given root_index to the LCA of the given subtree node indices.
/// Assumes the nodes are:
/// * Descendants of the given index.
/// * A non-empty array.
///
/// Note that the if the LCA is the root, the path will be empty (0 length).
fn get_path_to_lca(
    root_index: &NodeIndex,
    subtree_indices: &SortedLeafIndices<'_>,
) -> PathToBottom {
    if subtree_indices.is_empty() {
        panic!("Unexpected empty array.");
    }
    let first_index = *subtree_indices.first().expect("Unexpected empty array.");
    let lca = if subtree_indices.len() == 1 {
        first_index
    } else {
        first_index.get_lca(subtree_indices.last().expect("Unexpected empty array"))
    };
    root_index.get_path_to_descendant(lca)
}

/// Returns whether a root of a subtree has leaves on both sides. Assumes that all leaves are
/// descendants of the root.
fn has_leaves_on_both_sides(root_index: &NodeIndex, leaf_indices: &SortedLeafIndices<'_>) -> bool {
    if leaf_indices.is_empty() {
        return false;
    }
    split_leaves(root_index, leaf_indices).iter().all(|leaves_in_side| !leaves_in_side.is_empty())
}

impl UpdatedSkeletonTreeImpl {
    /// Finalize the skeleton bottom layer := the updated skeleton nodes created directly from the
    /// original skeleton and leaf modifications, without being dependant in any descendants
    /// (i.e., modified leaves, and unmodified nodes).
    pub(crate) fn finalize_bottom_layer<'a, 'ctx>(
        original_skeleton: &impl OriginalSkeletonTree<'a, 'ctx>,
        leaf_modifications: &LeafModifications<SkeletonLeaf>,
    ) -> UpdatedSkeletonNodeMap {
        leaf_modifications
            .iter()
            .filter(|(_, leaf)| !leaf.is_zero())
            .map(|(index, _)| (*index, UpdatedSkeletonNode::Leaf))
            .chain(original_skeleton.get_nodes().iter().filter_map(|(index, node)| match node {
                OriginalSkeletonNode::UnmodifiedSubTree(hash) => {
                    Some((*index, UpdatedSkeletonNode::UnmodifiedSubTree(*hash)))
                }
                OriginalSkeletonNode::Binary | OriginalSkeletonNode::Edge(_) => None,
            }))
            .collect()
    }

    /// Finalize the tree middle layers (i.e., not the bottom layer defined above).
    pub(crate) fn finalize_middle_layers<'a, 'ctx>(
        &mut self,
        original_skeleton: &mut impl OriginalSkeletonTree<'a, 'ctx>,
    ) -> TempSkeletonNode {
        let sorted_leaf_indices = original_skeleton.get_sorted_leaf_indices();
        if original_skeleton.get_nodes().is_empty() {
            self.update_node_in_empty_tree(&NodeIndex::ROOT, &sorted_leaf_indices)
        } else {
            self.update_node_in_nonempty_tree(
                &NodeIndex::ROOT,
                original_skeleton.get_nodes_mut(),
                &sorted_leaf_indices,
            )
        }
    }

    /// Updates the originally empty Patricia-Merkle tree rooted at the given index, with leaf
    /// modifications (already updated in the skeleton mapping) in the given leaf_indices.
    /// Returns the root temporary skeleton node as inferred from the subtree.
    pub(crate) fn update_node_in_empty_tree(
        &mut self,
        root_index: &NodeIndex,
        leaf_indices: &SortedLeafIndices<'_>,
    ) -> TempSkeletonNode {
        if root_index.is_leaf() {
            // Leaf. As this is an empty tree, the leaf *should* be new.
            assert!(
                leaf_indices.len() == 1
                    && leaf_indices.first().expect("Unexpected empty array.") == root_index,
                "Unexpected leaf index (root_index={root_index:?}, leaf_indices={leaf_indices:?})."
            );
            if !self.skeleton_tree.contains_key(root_index) {
                // "Deletion" of an original empty leaf (as non-zero leaf modifications are
                // finalized in `finalize_bottom_layer`). Supported but not
                // expected.
                return TempSkeletonNode::Empty;
            }
            return TempSkeletonNode::Leaf;
        }

        if has_leaves_on_both_sides(root_index, leaf_indices) {
            // Binary node.
            let [left_indices, right_indices] = split_leaves(root_index, leaf_indices);
            let [left_child_index, right_child_index] = root_index.get_children_indices();
            let left_child = self.update_node_in_empty_tree(&left_child_index, &left_indices);
            let right_child = self.update_node_in_empty_tree(&right_child_index, &right_indices);
            return self.node_from_binary_data(root_index, &left_child, &right_child);
        }

        // Edge node.
        let path_to_lca = get_path_to_lca(root_index, leaf_indices);
        let bottom_index = path_to_lca.bottom_index(*root_index);
        let bottom = self.update_node_in_empty_tree(&bottom_index, leaf_indices);
        self.node_from_edge_data(&path_to_lca, &bottom_index, &bottom)
    }

    /// Updates the Patricia tree rooted at the given index, with the given leaves; returns the
    /// root.
    pub(crate) fn update_node_in_nonempty_tree(
        &mut self,
        root_index: &NodeIndex,
        original_skeleton: &mut OriginalSkeletonNodeMap,
        leaf_indices: &SortedLeafIndices<'_>,
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

        // Not a leaf or an unmodified node.
        let original_node = *original_skeleton
            .get(root_index)
            .unwrap_or_else(|| panic!("Node {root_index:?} not found."));

        if leaf_indices.is_empty() {
            match original_node {
                OriginalSkeletonNode::Binary => unreachable!(
                    "Index {root_index:?} is an original Binary node without leaf modifications -
                    it should be an unmodified subtree instead."
                ),
                OriginalSkeletonNode::Edge(_) | OriginalSkeletonNode::UnmodifiedSubTree(_) => {
                    return TempSkeletonNode::Original(original_node);
                }
            }
        };

        match original_node {
            OriginalSkeletonNode::UnmodifiedSubTree(_) => {
                unreachable!(
                    "An unmodified subtree can't have any leaf_modifications in its subtree."
                )
            }
            OriginalSkeletonNode::Binary => {
                let [left_indices, right_indices] = split_leaves(root_index, leaf_indices);
                let [left_child_index, right_child_index] = root_index.get_children_indices();
                let left = self.update_node_in_nonempty_tree(
                    &left_child_index,
                    original_skeleton,
                    &left_indices,
                );
                let right = self.update_node_in_nonempty_tree(
                    &right_child_index,
                    original_skeleton,
                    &right_indices,
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
                    OriginalSkeletonNode::UnmodifiedSubTree(_) => {
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
                        "bottom {bottom_index:?} is a non-empty leaf but doesn't appear in the \
                         skeleton."
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
                self.skeleton_tree.insert(*bottom_index, UpdatedSkeletonNode::Binary);
                OriginalSkeletonNode::Edge(*path)
            }
            OriginalSkeletonNode::UnmodifiedSubTree(_) => OriginalSkeletonNode::Edge(*path),
        })
    }

    /// Update an original subtree rooted with an edge node.
    fn update_edge_node(
        &mut self,
        root_index: &NodeIndex,
        path_to_bottom: &PathToBottom,
        original_skeleton: &mut OriginalSkeletonNodeMap,
        leaf_indices: &SortedLeafIndices<'_>,
    ) -> TempSkeletonNode {
        let [left_child_index, right_child_index] = root_index.get_children_indices();
        let [left_indices, right_indices] = split_leaves(root_index, leaf_indices);
        let was_left_nonempty = path_to_bottom.is_left_descendant();
        if (!right_indices.is_empty() && was_left_nonempty)
            || (!left_indices.is_empty() && !was_left_nonempty)
        {
            // The root has a new leaf on its originally empty subtree.
            let (
                nonempty_subtree_child_index,
                nonempty_subtree_leaf_indices,
                empty_subtree_child_index,
                empty_subtree_leaf_indices,
            ) = if was_left_nonempty {
                (left_child_index, left_indices, right_child_index, right_indices)
            } else {
                (right_child_index, right_indices, left_child_index, left_indices)
            };

            // 1. Handle the originally non-empty subtree, replacing the root with the child in the
            //    direction of the edge.
            if u8::from(path_to_bottom.length) > 1 {
                // Bottom is not a child of the root, removing the first edge returns a valid new
                // edge node. Inject the new node to the original skeleton as if it was in it
                // originally (fake original).
                let fake_original_child_node = OriginalSkeletonNode::Edge(
                    path_to_bottom
                        .remove_first_edges(EdgePathLength::ONE)
                        .expect("Original Edge node is unexpectedly trivial"),
                );
                original_skeleton.insert(nonempty_subtree_child_index, fake_original_child_node);
            };

            let orig_nonempty_subtree_child = self.update_node_in_nonempty_tree(
                &nonempty_subtree_child_index,
                original_skeleton,
                &nonempty_subtree_leaf_indices,
            );

            // 2. Handle the originally empty subtree.
            let orig_empty_subtree_child = self
                .update_node_in_empty_tree(&empty_subtree_child_index, &empty_subtree_leaf_indices);
            let (left, right) = if was_left_nonempty {
                (orig_nonempty_subtree_child, orig_empty_subtree_child)
            } else {
                (orig_empty_subtree_child, orig_nonempty_subtree_child)
            };

            return self.node_from_binary_data(root_index, &left, &right);
        }

        // All leaves are on the edge's subtree - they have a non-trivial common path with the edge.
        // Create a new edge to the LCA of the leaves and the bottom.
        let path_to_leaves_lca = get_path_to_lca(root_index, leaf_indices);
        let leaves_lca_index = path_to_leaves_lca.bottom_index(*root_index);

        let bottom_index = path_to_bottom.bottom_index(*root_index);
        let path_to_new_bottom = get_path_to_lca(
            root_index,
            &SortedLeafIndices::new(&mut [leaves_lca_index, bottom_index]),
        );

        let new_bottom_index = path_to_new_bottom.bottom_index(*root_index);
        if new_bottom_index == bottom_index {
            //  All leaf_indices are in the bottom_node subtree.
            assert_eq!(&path_to_new_bottom, path_to_bottom);
        } else {
            // Inject the new node to the original skeleton as if it was in it
            // originally (fake original).
            let fake_original_new_bottom_node = OriginalSkeletonNode::Edge(
                path_to_bottom
                    .remove_first_edges(path_to_new_bottom.length)
                    .expect("Unexpectedly failed to remove first edges."),
            );

            original_skeleton.insert(new_bottom_index, fake_original_new_bottom_node);
        }

        let bottom =
            self.update_node_in_nonempty_tree(&new_bottom_index, original_skeleton, leaf_indices);

        self.node_from_edge_data(&path_to_new_bottom, &new_bottom_index, &bottom)
    }

    pub(crate) fn create_unmodified<'a, 'ctx>(
        original_skeleton: &impl OriginalSkeletonTree<'a, 'ctx>,
    ) -> UpdatedSkeletonTreeResult<Self> {
        let original_root_node = original_skeleton
            .get_nodes()
            .get(&NodeIndex::ROOT)
            .ok_or(UpdatedSkeletonTreeError::MissingNode(NodeIndex::ROOT))?;
        let OriginalSkeletonNode::UnmodifiedSubTree(root_hash) = original_root_node else {
            panic!("A root of tree without modifications is expected to be an unmodified node.")
        };

        Ok(Self {
            skeleton_tree: HashMap::from([(
                NodeIndex::ROOT,
                UpdatedSkeletonNode::UnmodifiedSubTree(*root_hash),
            )]),
        })
    }
}
