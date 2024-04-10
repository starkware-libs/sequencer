use crate::patricia_merkle_tree::types::{EdgePath, EdgePathLength, NodeIndex, PathToBottom};
use crate::types::Felt;
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
        NodeIndex(Felt::from(node_index)),
        PathToBottom {
            path: EdgePath(Felt::from(path)),
            length: EdgePathLength(length),
        },
    );
    let expected = NodeIndex(Felt::from(expected));
    assert_eq!(bottom_index, expected);
}
