use ethnum::U256;
use rstest::rstest;
use starknet_rust_core::types::{EdgeNode, MerkleNode};
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom, Preimage};

#[rstest]
#[case(PathToBottom::from("1011"), 1, PathToBottom::from("011"))]
#[case(PathToBottom::from("1011"), 2, PathToBottom::from("11"))]
#[case(PathToBottom::from("1011"), 3, PathToBottom::from("1"))]
#[case(PathToBottom::from("1011"), 4, PathToBottom::new(U256::ZERO.into(), EdgePathLength::new(0).unwrap()).unwrap())]
#[should_panic]
#[case(PathToBottom::from("1011"), 5, PathToBottom::from("0"))]
fn test_remove_first_edges(
    #[case] path_to_bottom: PathToBottom,
    #[case] n_edges: u8,
    #[case] expected: PathToBottom,
) {
    assert_eq!(
        path_to_bottom.remove_first_edges(EdgePathLength::new(n_edges).unwrap()).unwrap(),
        expected
    );
}

#[test]
#[should_panic(expected = "EdgeNode length 256 exceeds u8::MAX")]
fn test_preimage_from_edge_merkle_node_length_exceeds_u8() {
    let merkle_node =
        MerkleNode::EdgeNode(EdgeNode { child: Felt::ONE, path: Felt::ZERO, length: 256 });

    let _ = Preimage::from(&merkle_node);
}

#[test]
#[should_panic(expected = "Failed to create PathToBottom from MerkleNode edge")]
fn test_preimage_from_edge_merkle_node_path_mismatch() {
    // Path 0b1111 (4 bits) with length 2 should fail - path is too long for the stated length.
    let merkle_node = MerkleNode::EdgeNode(EdgeNode {
        child: Felt::ONE,
        path: Felt::from(0b1111_u128),
        length: 2,
    });

    let _ = Preimage::from(&merkle_node);
}
