use crate::block_committer::input::StarknetStorageKey;
use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::types::TreeHeight;
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
fn test_starknet_storage_key_to_node_index(
    #[case] leaf_index: u128,
    #[case] tree_height: u8,
    #[case] expected_node_index: u128,
) {
    assert!(
        leaf_index < 2_u128.pow(tree_height.into()),
        "Invalid test arguments. The node index must be smaller than the number of nodes."
    );
    let actual = NodeIndex::from_starknet_storage_key(
        &StarknetStorageKey(Felt::from(leaf_index)),
        &TreeHeight(tree_height),
    );

    assert_eq!(actual, expected_node_index.into());
}
