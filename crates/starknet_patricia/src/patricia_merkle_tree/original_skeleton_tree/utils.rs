use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

/// Returns the height of the node with the given index.
pub(crate) fn get_node_height(index: &NodeIndex) -> SubTreeHeight {
    SubTreeHeight::new(SubTreeHeight::ACTUAL_HEIGHT.0 + 1 - index.bit_length())
}

/// Splits leaf_indices into two arrays according to the given root: the left child leaves and
/// the right child leaves. Assumes that all leaves are descendants of the root.
pub(crate) fn split_leaves<'a>(
    root_index: &NodeIndex,
    leaf_indices: &SortedLeafIndices<'a>,
) -> [SortedLeafIndices<'a>; 2] {
    if leaf_indices.is_empty() {
        return [SortedLeafIndices::default(), SortedLeafIndices::default()];
    }

    let root_height = get_node_height(root_index);
    let assert_descendant = |leaf_index: &NodeIndex| {
        if (*leaf_index >> u8::from(root_height)) != *root_index {
            panic!(
                "Leaf {leaf_index:?} is not a descendant of the root {root_index:?} (root \
                 height={root_height:?})."
            );
        }
    };

    let first_leaf = leaf_indices.first().expect("Unexpected empty array.");
    assert_descendant(first_leaf);

    if leaf_indices.len() > 1 {
        assert_descendant(leaf_indices.last().expect("leaf_indices unexpectedly empty."));
    }

    let right_child_index = (*root_index << 1) + 1;
    let leftmost_index_in_right_subtree = right_child_index << (u8::from(root_height) - 1);
    let leaves_split = leaf_indices.bisect_left(&leftmost_index_in_right_subtree);
    leaf_indices.divide_at_index(leaves_split)
}
