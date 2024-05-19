use super::split_leaves;
use crate::patricia_merkle_tree::test_utils::get_random_u256;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use ethnum::{uint, U256};
use rstest::rstest;

/// Creates an array of increasing random U256 numbers, with jumps of up to 'jump' between two
/// consecutive numbers.
fn create_increasing_random_array(size: usize, start: U256, jump: U256) -> Vec<U256> {
    let size_u256: U256 = size.try_into().unwrap();
    assert!(jump > 0 && start + jump * size_u256 < U256::MAX);
    let mut ret: Vec<U256> = Vec::with_capacity(size);
    let mut low = start;
    for i in 0..size {
        ret.push(get_random_u256(low, low + jump));
        low = ret[i] + 1;
    }
    ret
}

#[rstest]
#[case::small_tree_one_side(
    3_u8, U256::ONE, &[uint!("8"), uint!("10"), uint!("11")], 
    &[uint!("8"), uint!("10"), uint!("11")], &[]
)]
#[case::small_tree_two_sides(
    3_u8, U256::ONE, &[uint!("8"), uint!("10"), uint!("14")], 
    &[uint!("8"), uint!("10")], &[uint!("14")]
)]
#[should_panic]
#[case::small_tree_wrong_height(
    3_u8, U256::ONE, &[uint!("8"), uint!("10"), uint!("16")], &[], &[]
)]
#[should_panic]
#[case::small_tree_not_descendant(
    3_u8, uint!("2"), &[uint!("8"), uint!("10"), uint!("14")], &[], &[]
)]
fn test_split_leaves(
    #[case] tree_height: u8,
    #[case] root_index: U256,
    #[case] leaf_indices: &[U256],
    #[case] expected_left: &[U256],
    #[case] expected_right: &[U256],
) {
    let tree_height = TreeHeight::new(tree_height);
    let root_index = NodeIndex::new(root_index);
    let to_node_index = |arr: &[U256]| arr.iter().map(|i| NodeIndex::new(*i)).collect::<Vec<_>>();
    let leaf_indices = to_node_index(leaf_indices);
    let expected = [to_node_index(expected_left), to_node_index(expected_right)];
    assert_eq!(
        split_leaves(&tree_height, &root_index, &leaf_indices),
        expected
    );
}

#[rstest]
fn test_split_leaves_big_tree() {
    let left_leaf_indices =
        create_increasing_random_array(100, NodeIndex::FIRST_LEAF.into(), U256::ONE << 200);
    let right_leaf_indices = create_increasing_random_array(
        100,
        (U256::from(NodeIndex::FIRST_LEAF) + U256::from(NodeIndex::MAX_INDEX)) / 2 + 1,
        U256::ONE << 100,
    );
    test_split_leaves(
        TreeHeight::MAX.into(),
        NodeIndex::ROOT.into(),
        &[&left_leaf_indices[..], &right_leaf_indices[..]].concat(),
        left_leaf_indices.as_slice(),
        right_leaf_indices.as_slice(),
    )
}
