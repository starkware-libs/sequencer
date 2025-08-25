use ethnum::{uint, U256};
use rand::rngs::ThreadRng;
use rand::Rng;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::external_test_utils::get_random_u256;
use crate::patricia_merkle_tree::internal_test_utils::{random, small_tree_index_to_full};
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTree, SubTreeHeight};

#[rstest]
#[case(1, 1, 1, 3)]
#[case(1, 0, 2, 4)]
#[case(0xDAD, 0xFEE, 12, 0xDADFEE)]
#[case(0xDEAFBEE, 0xBFF, 16, 0xDEAFBEE0BFF)]
fn test_compute_bottom_index(
    #[case] node_index: u128,
    #[case] path: u128,
    #[case] length: u8,
    #[case] expected: u128,
) {
    let bottom_index = NodeIndex::compute_bottom_index(
        NodeIndex::from(node_index),
        &PathToBottom::new(path.into(), EdgePathLength::new(length).unwrap()).unwrap(),
    );
    let expected = NodeIndex::from(expected);
    assert_eq!(bottom_index, expected);
}

#[rstest]
#[case(uint!("1"), uint!("1"), uint!("1"))]
#[case(uint!("2"), uint!("5"), uint!("2"))]
#[case(uint!("5"), uint!("2"), uint!("2"))]
#[case(uint!("8"), uint!("10"), uint!("2"))]
#[case(uint!("9"), uint!("12"), uint!("1"))]
#[case(uint!("1"), uint!("2"), uint!("1"))]
#[case(uint!("2"), uint!("1"), uint!("1"))]
#[case(
    U256::from_words(1<<121, 0),
    U256::from_words(1<<123, 0),
    U256::from_words(1<<121, 0)
)]
#[case(
    U256::from_words(6<<121, 12109832645278165874326859176438297),
    U256::from_words(7<<121, 34269583569287659876592876529763453),
    uint!("3")
)]
#[case(
    uint!("3"),
    U256::from_str_hex("0xd2794ec01eb68c0f3334f2e9e6a3fee480249162fbb5b1cc491c1738368de89").unwrap(),
    uint!("3")
)]
fn test_get_lca(#[case] node_index: U256, #[case] other: U256, #[case] expected: U256) {
    let root_index = NodeIndex::new(node_index);
    let other_index = NodeIndex::new(other);
    let lca = root_index.get_lca(&other_index);
    let expected = NodeIndex::new(expected);
    assert_eq!(lca, expected);
}

#[rstest]
fn test_get_lca_big(mut random: ThreadRng) {
    let lca =
        NodeIndex::new(get_random_u256(&mut random, U256::ZERO, (NodeIndex::MAX >> 1).into()));

    let left_child = lca << 1;
    let right_child = left_child + 1;
    let mut random_extension = |index: NodeIndex| {
        let extension_bits = index.leading_zeros();
        let extension: u128 = random.gen_range(0..(1 << extension_bits));
        (index << extension_bits) + NodeIndex::new(U256::from(extension))
    };

    let left_descendant = random_extension(left_child);
    let right_descendant = random_extension(right_child);
    assert_eq!(left_descendant.get_lca(&right_descendant), lca);
}

#[rstest]
#[case(3, 3, 0, 0)]
#[case(2, 10, 2, 2)]
#[should_panic]
#[case(2, 3, 0, 0)]
#[should_panic]
#[case(2, 6, 0, 0)]
#[should_panic]
#[case(6, 2, 0, 0)]
fn test_get_path_to_descendant(
    #[case] root_index: u8,
    #[case] descendant: u8,
    #[case] expected_path: u8,
    #[case] expected_length: u8,
) {
    let root_index = NodeIndex::new(root_index.into());
    let descendant = NodeIndex::new(descendant.into());
    let path_to_bottom = root_index.get_path_to_descendant(descendant);
    assert_eq!(path_to_bottom.path, U256::from(expected_path).into());
    assert_eq!(path_to_bottom.length, EdgePathLength::new(expected_length).unwrap());
}

#[rstest]
fn test_get_path_to_descendant_big() {
    let root_index = NodeIndex::new(U256::from(rand::thread_rng().gen::<u128>()));
    let max_bits = NodeIndex::BITS - 128;
    let extension: u128 = rand::thread_rng().gen_range(0..1 << max_bits);
    let extension_index = NodeIndex::new(U256::from(extension));

    let descendant = (root_index << extension_index.bit_length()) + extension_index;
    let path_to_bottom = root_index.get_path_to_descendant(descendant);
    assert_eq!(path_to_bottom.path, extension.into());
    assert_eq!(path_to_bottom.length, EdgePathLength::new(extension_index.bit_length()).unwrap());
}

#[rstest]
fn test_nodeindex_to_felt_conversion() {
    let index = NodeIndex::MAX;
    assert!(Felt::try_from(index).is_err());
}

#[rstest]
fn test_felt_printing() {
    let felt = Felt::from(17_u8);
    assert_eq!(format!("{felt:?}"), "0x11");
}

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
