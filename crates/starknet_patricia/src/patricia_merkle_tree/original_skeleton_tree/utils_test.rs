use ethnum::{uint, U256};
use rand::rngs::ThreadRng;
use rand::Rng;
use rstest::rstest;

use super::split_leaves;
use crate::patricia_merkle_tree::external_test_utils::{
    as_fully_indexed,
    get_random_u256,
    small_tree_index_to_full,
};
use crate::patricia_merkle_tree::internal_test_utils::random;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

/// Creates an array of increasing random U256 numbers, with jumps of up to 'jump' between two
/// consecutive numbers.
fn create_increasing_random_array<R: Rng>(
    rng: &mut R,
    size: usize,
    start: U256,
    jump: U256,
) -> Vec<U256> {
    let size_u256: U256 = size.try_into().unwrap();
    assert!(jump > 0 && start + jump * size_u256 < U256::MAX);
    let mut ret: Vec<U256> = Vec::with_capacity(size);
    let mut low = start;
    for i in 0..size {
        ret.push(get_random_u256(rng, low, low + jump));
        low = ret[i] + 1;
    }
    ret
}

#[rstest]
#[case::small_tree_one_side(
    3_u8, U256::ONE, as_fully_indexed(subtree_height, [uint!("8"), uint!("10"), uint!("11")].into_iter()),
    &[uint!("8"), uint!("10"), uint!("11")], &[]
)]
#[case::small_tree_two_sides(
    3_u8, U256::ONE, as_fully_indexed(subtree_height, [uint!("8"), uint!("10"), uint!("14")].into_iter()),
    &[uint!("8"), uint!("10")], &[uint!("14")]
)]
#[should_panic]
#[case::small_tree_wrong_height(
    3_u8, U256::ONE, as_fully_indexed(subtree_height, [uint!("8"), uint!("10"), uint!("16")].into_iter()), &[], &[]
)]
#[should_panic]
#[case::small_tree_not_descendant(
    3_u8, uint!("2"), as_fully_indexed(subtree_height, [uint!("8"), uint!("10"), uint!("14")].into_iter()), &[], &[]
)]
fn test_split_leaves(
    #[case] subtree_height: u8,
    #[case] root_index: U256,
    #[case] mut leaf_indices: Vec<NodeIndex>,
    #[case] expected_left: &[U256],
    #[case] expected_right: &[U256],
) {
    let height = SubTreeHeight(subtree_height);
    let root_index = small_tree_index_to_full(root_index, height);
    let mut left_full_indices = as_fully_indexed(subtree_height, expected_left.iter().copied());
    let mut right_full_indices = as_fully_indexed(subtree_height, expected_right.iter().copied());

    let expected = [
        SortedLeafIndices::new(&mut left_full_indices),
        SortedLeafIndices::new(&mut right_full_indices),
    ];
    assert_eq!(split_leaves(&root_index, &SortedLeafIndices::new(&mut leaf_indices)), expected);
}

#[rstest]
fn test_split_leaves_big_tree(mut random: ThreadRng) {
    let left_leaf_indices = create_increasing_random_array(
        &mut random,
        100,
        NodeIndex::FIRST_LEAF.into(),
        U256::ONE << 200,
    );
    let right_leaf_indices = create_increasing_random_array(
        &mut random,
        100,
        (U256::from(NodeIndex::FIRST_LEAF) + U256::from(NodeIndex::MAX)) / 2 + 1,
        U256::ONE << 100,
    );
    test_split_leaves(
        SubTreeHeight::ACTUAL_HEIGHT.into(),
        NodeIndex::ROOT.into(),
        [
            left_leaf_indices.clone().into_iter().map(NodeIndex::new).collect::<Vec<NodeIndex>>(),
            right_leaf_indices.clone().into_iter().map(NodeIndex::new).collect(),
        ]
        .concat(),
        left_leaf_indices.as_slice(),
        right_leaf_indices.as_slice(),
    )
}
