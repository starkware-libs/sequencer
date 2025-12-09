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

fn contract_state_leaf() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from_hex_unchecked(
            "0x7fdeb85518534a06e6b50c2ccdea7bbf3d47c607a9b36fbf690c41274976950",
        )),
        data: NodeData::Leaf(IndexLayoutContractState(ContractState {
            class_hash: ClassHash(Felt::from(1)),
            storage_root_hash: HashOutput(Felt::from(2)),
            nonce: Nonce(Felt::from(3)),
        })),
    })
}

fn compiled_class_hash_leaf() -> IndexFilledNode<IndexLayoutCompiledClassHash> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from_hex_unchecked(
            "0x2f57789e16766426fe7f33b3ec589957350be6f186a75fe2b3ee0503a421323",
        )),
        data: NodeData::Leaf(IndexLayoutCompiledClassHash(CompiledClassHash(Felt::from(1)))),
    })
}

fn starknet_storage_value_leaf() -> IndexFilledNode<IndexLayoutStarknetStorageValue> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Leaf(IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from(1)))),
    })
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
            // 1...1 19 times followed by 001
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from(1_u128 << 19)),
                EdgePathLength::new(22).unwrap(),
            )
            .unwrap(),
        }),
    })
}

fn edge_node_long_path() -> IndexFilledNode<IndexLayoutContractState> {
    IndexFilledNode(FilledNode {
        hash: HashOutput(Felt::from(1)),
        data: NodeData::Edge(EdgeData {
            bottom_data: (),
            // 0..0 path of length 250
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from_words(0_u128, 0_u128)),
                EdgePathLength::new(250).unwrap(),
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
#[case::index_layout_contract_state_leaf(contract_state_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_compiled_class_hash_leaf(compiled_class_hash_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_starknet_storage_value_leaf(starknet_storage_value_leaf(), IndexNodeContext { is_leaf: true })]
#[case::index_layout_binary_node(binary_node(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_short_path(edge_node_short_path_len_3(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_short_path(edge_node_short_path_len_10(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_path_divisible_by_8(edge_node_path_divisible_by_8(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_path_not_divisible_by_8(edge_node_path_not_divisible_by_8(), IndexNodeContext { is_leaf: false })]
#[case::index_layout_edge_node_long_path(edge_node_long_path(), IndexNodeContext { is_leaf: false })]
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
