use ethnum::U256;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::internal_test_utils::small_tree_index_to_full;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::traversal::SubTree;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

/// case::single_right_child
///     1
///      \
///       3
///
/// Bottom subtree:
///       3
///
/// case::single_left_child
///     1
///    /
///   2
///
/// Bottom subtree:
///       2
///
/// case::missing_nodes
///
///       1
///      /
///     *
///    /
///   4
///  /  \
/// 8   9
///
/// Bottom subtree:
///
///    4
///   /  \
///  8    9
///
/// case::long_left_path
///
///              1
///             /
///            *
///           /
///         ...
///         /
///
/// NodeIndex::FIRST_LEAF
///
/// Bottom subtree:
///
///  NodeIndex::FIRST_LEAF
///
/// case::long_right_path
///
///              1
///               \
///                *
///                 \
///                 ...
///
///                    \
///                    NodeIndex::MAX
///
/// Bottom subtree:
///
///    NodeIndex::MAX
///
/// case::should_delete_new_leaf
///
///           1
///          / \
///         2   new
///
/// Bottom subtree:
///
///      2
///
/// case::should_delete_new_leafs
///
///            1
///         /     \
///        ^       /
///       / \     /
///      4   5   6
///     / \  / \  /
///    8  9 10 11 12
///   new new    new
///
/// Bottom subtree:
///
///      5
///     / \
///   11  10
#[rstest]
#[case::single_right_child(
        SubTreeHeight(1),
        &[U256::from(3_u128)],
        PathToBottom::new(EdgePath(U256::ONE), EdgePathLength::new(1).unwrap()).unwrap(),
        &[U256::from(3_u128)],
        U256::from(3_u128),
    )]
#[case::single_left_child(
    SubTreeHeight(1),
    &[U256::from(2_u128)],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(1).unwrap()).unwrap(),
    &[U256::from(2_u128)],
    U256::from(2_u128),
)]
#[case::missing_nodes(
    SubTreeHeight(3),
    &[U256::from(8_u128),U256::from(9_u128)],
    PathToBottom::new(EdgePath(U256::ZERO),EdgePathLength::new(2).unwrap()).unwrap(),
    &[U256::from(8_u128),U256::from(9_u128)],
    U256::from(4_u128),
)]
#[case::long_left_path(
    SubTreeHeight::ACTUAL_HEIGHT,
    &[NodeIndex::FIRST_LEAF.0],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0).unwrap()).unwrap(),
    &[NodeIndex::FIRST_LEAF.0],
    NodeIndex::FIRST_LEAF.0,
)]
#[case::long_right_path(
    SubTreeHeight::ACTUAL_HEIGHT,
    &[NodeIndex::MAX.0],
    PathToBottom::new(EdgePath(NodeIndex::MAX.0 >> 1), EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0).unwrap()).unwrap(),
    &[NodeIndex::MAX.0],
    NodeIndex::MAX.0,
    )]
#[case::should_delete_new_leaf(
    SubTreeHeight(1),
    &[U256::from(2_u128), U256::from(3_u128)],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(1).unwrap()).unwrap(),
    &[U256::from(2_u128)],
    U256::from(2_u128),
)]
#[case::should_delete_new_leafs(
    SubTreeHeight(3),
    &[U256::from(8_u128), U256::from(9_u128), U256::from(10_u128), U256::from(11_u128), U256::from(12_u128)],
    PathToBottom::new(EdgePath(U256::ONE), EdgePathLength::new(2).unwrap()).unwrap(),
    &[U256::from(10_u128), U256::from(11_u128)],
    U256::from(5_u128),
)]
fn test_get_bottom_subtree(
    #[case] height: SubTreeHeight,
    #[case] sorted_leaf_indices: &[U256],
    #[case] path_to_bottom: PathToBottom,
    #[case] expected_sorted_leaf_indices: &[U256],
    #[case] expected_root_index: U256,
) {
    // Cast the input to the correct type for subtree.
    let root_index = small_tree_index_to_full(U256::ONE, height);

    let mut leaf_indices = sorted_leaf_indices
        .iter()
        .map(|&idx| small_tree_index_to_full(idx, height))
        .collect::<Vec<_>>();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut leaf_indices);
    // Cast the expected output to the correct type for subtree.
    let mut expected_leaf_indices = expected_sorted_leaf_indices
        .iter()
        .map(|&idx| small_tree_index_to_full(idx, height))
        .collect::<Vec<_>>();
    let expected_sorted_leaf_indices = SortedLeafIndices::new(&mut expected_leaf_indices);

    let expected_previously_empty_leaf_indices = create_previously_empty_leaf_indices(
        sorted_leaf_indices.get_indices(),
        expected_sorted_leaf_indices.get_indices(),
    );

    // Create the input Subtree.
    let tree = SubTree { sorted_leaf_indices, root_index, root_hash: HashOutput(Felt::ONE) };

    // Get the bottom subtree.
    let (subtree, previously_empty_leaf_indices) =
        tree.get_bottom_subtree(&path_to_bottom, HashOutput(Felt::TWO));

    let expected_root_index = small_tree_index_to_full(expected_root_index, height);

    // Create the expected subtree.
    let expected_subtree = SubTree {
        sorted_leaf_indices: expected_sorted_leaf_indices,
        root_index: expected_root_index,
        root_hash: HashOutput(Felt::TWO),
    };
    assert_eq!(previously_empty_leaf_indices, expected_previously_empty_leaf_indices);
    assert_eq!(subtree, expected_subtree);
}

fn create_previously_empty_leaf_indices<'a>(
    tree_leaf_indices: &'a [NodeIndex],
    subtree_leaf_indices: &'a [NodeIndex],
) -> Vec<&'a NodeIndex> {
    tree_leaf_indices.iter().filter(|idx| !subtree_leaf_indices.contains(idx)).collect()
}
