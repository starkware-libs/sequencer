use crate::hash::hash_trait::{HashFunction, HashInputPair, HashOutput};
use crate::hash::pedersen::PedersenHashFunction;
use crate::patricia_merkle_tree::filled_node::{BinaryData, NodeData};
use crate::patricia_merkle_tree::types::EdgeData;
use crate::patricia_merkle_tree::types::TreeHashFunction;
use crate::patricia_merkle_tree::types::{
    EdgePath, EdgePathLength, NodeIndex, PathToBottom, TreeHashFunctionImpl,
};
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

#[rstest]
#[case(Felt::ONE, Felt::TWO, Felt::from_hex("0x5bb9440e27889a364bcb678b1f679ecd1347acdedcbf36e83494f857cc58026").unwrap())]
#[case(Felt::from(0xBE_u128), Felt::from(0xA0BEE_u128), Felt::from_hex("0x4e8f149d7d5adb77a8c85b631a3acb6fb9aa5ecb06ea4ec105753629243e380").unwrap())]
#[case(Felt::from(0x1234_u128), Felt::from(0xABCD_u128), Felt::from_hex("0x615bb8d47888d2987ad0c63fc06e9e771930986a4dd8adc55617febfcf3639e").unwrap())]
fn test_tree_hash_function_impl_binary_node(
    #[case] left_hash: Felt,
    #[case] right_hash: Felt,
    #[case] expected_hash: Felt,
) {
    let hash_output = TreeHashFunctionImpl::<PedersenHashFunction>::compute_node_hash(
        &NodeData::Binary(BinaryData {
            left_hash: HashOutput(left_hash),
            right_hash: HashOutput(right_hash),
        }),
    );
    assert_eq!(
        hash_output,
        PedersenHashFunction::compute_hash(HashInputPair(left_hash, right_hash))
    );
    assert_eq!(hash_output, HashOutput(expected_hash));
}

#[rstest]
#[case(Felt::ONE, Felt::TWO, 3,  Felt::from_hex("0x5bb9440e27889a364bcb678b1f679ecd1347acdedcbf36e83494f857cc58029").unwrap())]
#[case(Felt::from(0xBE_u128), Felt::from(0xA0BEE_u128), 0xBB,  Felt::from_hex("0x4e8f149d7d5adb77a8c85b631a3acb6fb9aa5ecb06ea4ec105753629243e43b").unwrap())]
#[case(Felt::from(0x1234ABCD_u128),Felt::from(42_u128),6, Felt::from_hex("0x1d937094c09b5f8e26a662d21911871e3cbc6858d55cc49af9848ea6fed4e9").unwrap())]
fn test_tree_hash_function_impl_edge_node(
    #[case] bottom_hash: Felt,
    #[case] edge_path: Felt,
    #[case] length: u8,
    #[case] expected_hash: Felt,
) {
    let hash_output = TreeHashFunctionImpl::<PedersenHashFunction>::compute_node_hash(
        &NodeData::Edge(EdgeData {
            bottom_hash: HashOutput(bottom_hash),
            path_to_bottom: PathToBottom {
                path: EdgePath(edge_path),
                length: EdgePathLength(length),
            },
        }),
    );
    let direct_hash_computation = HashOutput(
        PedersenHashFunction::compute_hash(HashInputPair(bottom_hash, edge_path)).0
            + Felt::from(length),
    );
    assert_eq!(hash_output, HashOutput(expected_hash));
    assert_eq!(hash_output, direct_hash_computation);
}
