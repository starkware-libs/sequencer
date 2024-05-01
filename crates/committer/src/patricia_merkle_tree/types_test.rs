use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::types::NodeIndex;
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
