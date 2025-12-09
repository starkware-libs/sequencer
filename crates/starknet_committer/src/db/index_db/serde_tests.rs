use ethnum::U256;
use rstest::rstest;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyDeserializationContext};
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
};
use crate::db::index_db::types::{IndexFilledNode, IndexNodeContext};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Converts leaf node data from NodeData<L, HashOutput> to NodeData<L, ()>.
///
/// Useful for using the same data for computing the hash and creating an index layout leaf
/// instance.
fn index_leaf_data_from_hash_data<L: Leaf>(data: NodeData<L, HashOutput>) -> NodeData<L, ()> {
    match data {
        NodeData::Binary(_) | NodeData::Edge(_) => {
            unreachable!("this helper is intended for leaf-only test data")
        }
        NodeData::Leaf(leaf) => NodeData::Leaf(leaf),
    }
}

fn contract_state_leaf() -> IndexFilledNode<IndexLayoutContractState> {
    let data = NodeData::Leaf(IndexLayoutContractState(ContractState {
        class_hash: ClassHash(Felt::from(1)),
        storage_root_hash: HashOutput(Felt::from(2)),
        nonce: Nonce(Felt::from(3)),
    }));
    let hash = TreeHashFunctionImpl::compute_node_hash(&data);
    IndexFilledNode(FilledNode { hash, data: index_leaf_data_from_hash_data(data) })
}

fn compiled_class_hash_leaf() -> IndexFilledNode<IndexLayoutCompiledClassHash> {
    let data = NodeData::Leaf(IndexLayoutCompiledClassHash(CompiledClassHash(Felt::from(1))));
    let hash = TreeHashFunctionImpl::compute_node_hash(&data);
    IndexFilledNode(FilledNode { hash, data: index_leaf_data_from_hash_data(data) })
}

fn starknet_storage_value_leaf() -> IndexFilledNode<IndexLayoutStarknetStorageValue> {
    let data = NodeData::Leaf(IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from(1))));
    let hash = TreeHashFunctionImpl::compute_node_hash(&data);
    IndexFilledNode(FilledNode { hash, data: index_leaf_data_from_hash_data(data) })
}

fn starknet_storage_value_leaf_96_bits() -> IndexLayoutStarknetStorageValue {
    // 2^96 (12 bytes, under the 27 nibbles threshold)
    IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from(1_u128 << 95)))
}

fn starknet_storage_value_leaf_136_bits() -> IndexLayoutStarknetStorageValue {
    // 2^136 (reaching the 34 nibbles / 17 bytes serialization threshold)
    IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from_bytes_be(&[
        0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8,
        128_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8,
        0_u8, 0_u8,
    ])))
}

fn binary_node() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Binary(BinaryData { left_data: (), right_data: () }),
    })
}

fn edge_node_short_path_len_3() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 110, right, right, left
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from(6_u128)),
                EdgePathLength::new(3).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_short_path_len_10() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 0...0 seven times followed by 110
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from(6_u128)),
                EdgePathLength::new(10).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_path_divisible_by_8() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            path_to_bottom: PathToBottom::new(
                // 1...1 24 times
                EdgePath(U256::from((1_u128 << 24) - 1)),
                EdgePathLength::new(24).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_path_not_divisible_by_8() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 000 followed by 1...1 19 times
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from((1_u128 << 19) - 1)),
                EdgePathLength::new(22).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_long_zero_path() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 0...0 path of length 250
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from_words(0_u128, 0_u128)),
                EdgePathLength::new(250).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_long_non_zero_path() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 1 followed by 250 zeros path of length 251
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from_words(1 >> 122, 0_u128)),
                EdgePathLength::new(251).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn extract_leaf<L: Leaf>(node: IndexFilledNode<L>) -> L {
    if let NodeData::Leaf(leaf) = node.0.data {
        leaf
    } else {
        unreachable!("attempted to extract a leaf from a non-leaf node");
    }
}

#[rstest]
#[case::index_layout_contract_state(extract_leaf(contract_state_leaf()))]
#[case::index_layout_compiled_class_hash(extract_leaf(compiled_class_hash_leaf()))]
#[case::index_layout_starknet_storage_value(extract_leaf(starknet_storage_value_leaf()))]
fn test_index_layout_leaf_serde<L: Leaf>(#[case] leaf: L) {
    let serialized = leaf.serialize().unwrap();
    let deserialized = L::deserialize(&serialized, &EmptyDeserializationContext).unwrap();
    assert_eq!(leaf, deserialized);
}

#[rstest]
#[case(extract_leaf(contract_state_leaf()), DbValue(vec![1, 2, 3]))]
#[case(extract_leaf(compiled_class_hash_leaf()), DbValue(vec![1]))]
#[case(extract_leaf(starknet_storage_value_leaf()), DbValue(vec![1]))]
// We are serializing 2^96. The 4 MSB of the first byte are the chooser. For values >= 16 but under
// 27 nibbles, the chooser is the number of bytes. In this case, the first byte will be 11000000
// (chooser 12, i.e. we need 12 bytes) followed by the value.
#[case(starknet_storage_value_leaf_96_bits(), DbValue(vec![192, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]))]
// We are serializing 2^136, which exceeds the 34 nibbles threshold where the encoding utilizes the
// full 32 bytes. This case is marked by chooser = 15, followed by the value, starting immediately
// after the chooser (hence the first 116 bits after the chooser are 0).
#[case(starknet_storage_value_leaf_136_bits(), DbValue(vec![
    240, 0, 0, 0, 0, 0, 0, 0, 
    0, 0, 0, 0, 0, 0, 0, 128,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0
]))]
fn test_leaf_serialization_regression<L: Leaf>(
    #[case] leaf: L,
    #[case] expected_serialize: DbValue,
) {
    let actual_serialize = leaf.serialize().unwrap();
    assert_eq!(actual_serialize, expected_serialize);
}

#[rstest]
#[case::index_layout_contract_state_leaf(contract_state_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_compiled_class_hash_leaf(compiled_class_hash_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_starknet_storage_value_leaf(starknet_storage_value_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_binary_node(binary_node(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_short_path(edge_node_short_path_len_3(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_short_path(edge_node_short_path_len_10(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_path_divisible_by_8(edge_node_path_divisible_by_8(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_path_not_divisible_by_8(edge_node_path_not_divisible_by_8(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_long_zero_path(edge_node_long_zero_path(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_long_non_zero_path(edge_node_long_non_zero_path(), IndexNodeContext { is_leaf: false })]

fn test_index_layout_node_serde<L: Leaf>(
    #[case] node: IndexFilledNode<L>,
    #[case] deserialization_context: IndexNodeContext,
) where
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    let serialized = node.serialize().unwrap();
    let deserialized =
        IndexFilledNode::<L>::deserialize(&serialized, &deserialization_context).unwrap();
    assert_eq!(node, deserialized);
}
