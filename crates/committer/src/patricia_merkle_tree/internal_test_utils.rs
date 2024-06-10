use crate::felt::Felt;
use crate::patricia_merkle_tree::external_test_utils::get_random_u256;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;

use crate::patricia_merkle_tree::types::{NodeIndex, SubTreeHeight};

use ethnum::U256;
use rand::rngs::ThreadRng;
use rstest::{fixture, rstest};

impl From<u8> for SkeletonLeaf {
    fn from(value: u8) -> Self {
        Self::from(Felt::from(value))
    }
}

impl From<&str> for PathToBottom {
    fn from(value: &str) -> Self {
        Self::new(
            U256::from_str_radix(value, 2)
                .expect("Invalid binary string")
                .into(),
            EdgePathLength::new(
                (value.len() - if value.starts_with('+') { 1 } else { 0 })
                    .try_into()
                    .expect("String is too large"),
            )
            .expect("Invalid length"),
        )
        .expect("Illegal PathToBottom")
    }
}

#[fixture]
pub(crate) fn random() -> ThreadRng {
    rand::thread_rng()
}

impl NodeIndex {
    /// Assumes self represents an index in a smaller tree height. Returns a node index represents
    /// the same index in the starknet state tree as if the smaller tree was 'planted' at the lowest
    /// leftmost node from the root.
    pub(crate) fn from_subtree_index(subtree_index: Self, subtree_height: SubTreeHeight) -> Self {
        let height_diff = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
        let offset = (NodeIndex::ROOT << height_diff) - 1.into();
        subtree_index + (offset << (subtree_index.bit_length() - 1))
    }
}

pub(crate) fn small_tree_index_to_full(index: U256, height: SubTreeHeight) -> NodeIndex {
    NodeIndex::from_subtree_index(NodeIndex::new(index), height)
}

#[rstest]
#[should_panic]
#[case(U256::ZERO, U256::ZERO)]
#[case(U256::ZERO, U256::ONE)]
#[case(U256::ONE, U256::ONE << 128)]
#[case((U256::ONE<<128)-U256::ONE, U256::ONE << 128)]
#[case(U256::ONE<<128, (U256::ONE << 128)+U256::ONE)]
fn test_get_random_u256(mut random: ThreadRng, #[case] low: U256, #[case] high: U256) {
    let r = get_random_u256(&mut random, low, high);
    assert!(low <= r && r < high);
}

pub(crate) fn as_fully_indexed(
    subtree_height: u8,
    indices: impl Iterator<Item = U256>,
) -> Vec<NodeIndex> {
    indices
        .map(|index| small_tree_index_to_full(index, SubTreeHeight::new(subtree_height)))
        .collect()
}
