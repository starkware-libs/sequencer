use crate::block_committer::input::{ContractAddress, StarknetStorageKey};
use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::types::TreeHeight;

use ethnum::U256;
use rand::Rng;
use rstest::rstest;

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
        &PathToBottom {
            path: EdgePath(Felt::from(path)),
            length: EdgePathLength(length),
        },
    );
    let expected = NodeIndex::from(expected);
    assert_eq!(bottom_index, expected);
}

#[rstest]
#[case(0, 127, 2_u128.pow(127))]
#[case(15, 118, 2_u128.pow(118) | 15)]
#[case(0xDEADBEEF, 60, 2_u128.pow(60) | 0xDEADBEEF)]
fn test_cast_to_node_index(
    #[case] leaf_index: u128,
    #[case] tree_height: u8,
    #[case] expected_node_index: u128,
    #[values(true, false)] from_contract_address: bool,
) {
    assert!(
        leaf_index < 2_u128.pow(tree_height.into()),
        "Invalid test arguments. The node index must be smaller than the number of nodes."
    );
    let actual = if from_contract_address {
        NodeIndex::from_contract_address(
            &ContractAddress(Felt::from(leaf_index)),
            &TreeHeight(tree_height),
        )
    } else {
        NodeIndex::from_starknet_storage_key(
            &StarknetStorageKey(Felt::from(leaf_index)),
            &TreeHeight(tree_height),
        )
    };
    assert_eq!(actual, expected_node_index.into());
}

fn get_random_u256(min_trailing_zeros: u8) -> U256 {
    let msbits: U256 = rand::thread_rng().gen_range(0..u128::MAX).into();
    let lsbits: u128 = rand::thread_rng().gen();
    ((msbits << (128 - lsbits.leading_zeros())) + lsbits) >> min_trailing_zeros
}

#[rstest]
#[case(1, 1, 1)]
#[case(2, 5, 2)]
#[case(5, 2, 2)]
#[case(8, 10, 2)]
#[case(9, 12, 1)]
fn test_get_lca(#[case] node_index: u8, #[case] other: u8, #[case] expected: u8) {
    let root_index = NodeIndex(node_index.into());
    let other_index = NodeIndex(other.into());
    let lca = root_index.get_lca(&other_index);
    let expected = NodeIndex(expected.into());
    assert_eq!(lca, expected);
}

#[rstest]
fn test_get_lca_big() {
    let lca = NodeIndex(get_random_u256(5));

    let left_child = lca << 1;
    let right_child = left_child + 1.into();
    let random_extension = |index: NodeIndex| {
        let extension_bits = index.leading_zeros();
        let extension: u128 = rand::thread_rng().gen_range(0..(1 << extension_bits));
        (index << extension_bits) + NodeIndex(U256::from(extension))
    };

    let left_descendant = random_extension(left_child);
    let right_descendant = random_extension(right_child);
    assert_eq!(left_descendant.get_lca(&right_descendant), lca);
}
