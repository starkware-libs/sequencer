use std::collections::HashMap;

use async_recursion::async_recursion;
use rstest::rstest;
use rstest_reuse::apply;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::db_layout::TrieType;
use starknet_patricia::patricia_merkle_tree::external_test_utils::{MockIndexLayoutLeaf, MockLeaf};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::{FactDbFilledNode, FilledNode};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::{
    FactNodeDeserializationContext,
    PatriciaPrefix,
};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{create_db_key, DbHashMap, DbValue, Storage};
use starknet_types_core::felt::Felt;

use crate::db::create_original_skeleton_tests::case_helpers::CreateTreeCase;
use crate::db::create_original_skeleton_tests::{create_tree_cases, test_create_original_skeleton};
use crate::db::index_db::db::IndexNodeLayout;
use crate::db::index_db::types::IndexFilledNode;
use crate::hash_function::hash::TreeHashFunctionImpl;

pub async fn convert_facts_db_to_index_db<FactsLeaf, IndexLeaf>(
    storage: &mut MapStorage,
    root_hash: HashOutput,
    trie_type: Option<TrieType>,
) -> MapStorage
where
    FactsLeaf: Leaf + Into<IndexLeaf> + HasStaticPrefix<KeyContext = EmptyKeyContext>,
    IndexLeaf: Leaf + HasStaticPrefix<KeyContext = TrieType>,
    TreeHashFunctionImpl: TreeHashFunction<IndexLeaf>,
{
    let mut index_layout_storage = MapStorage(DbHashMap::new());
    let trie_type = trie_type.unwrap_or(TrieType::StorageTrie(ContractAddress::from(0_u128)));
    convert_facts_to_index_layout_inner::<FactsLeaf, IndexLeaf>(
        storage,
        &mut index_layout_storage,
        NodeIndex::ROOT,
        root_hash,
        &trie_type,
    )
    .await;
    index_layout_storage
}

#[async_recursion]
async fn convert_facts_to_index_layout_inner<FactsLeaf, IndexLeaf>(
    facts_storage: &mut MapStorage,
    index_layout_storage: &mut MapStorage,
    current_root_index: NodeIndex,
    current_root_hash: HashOutput,
    trie_type: &TrieType,
) where
    FactsLeaf: Leaf + Into<IndexLeaf> + HasStaticPrefix<KeyContext = EmptyKeyContext>,
    IndexLeaf: Leaf + HasStaticPrefix<KeyContext = TrieType>,
    TreeHashFunctionImpl: TreeHashFunction<IndexLeaf>,
{
    let db_prefix = if current_root_index.is_leaf() {
        FactsLeaf::get_static_prefix(&EmptyKeyContext)
    } else {
        PatriciaPrefix::InnerNode.into()
    };
    let facts_db_key = create_db_key(db_prefix, &current_root_hash.0.to_bytes_be());
    let filled_root_raw: DbValue = facts_storage.get(&facts_db_key).await.unwrap().unwrap();

    let facts_filled_root = FactDbFilledNode::<FactsLeaf>::deserialize(
        &filled_root_raw,
        &FactNodeDeserializationContext {
            node_hash: current_root_hash,
            is_leaf: current_root_index.is_leaf(),
        },
    )
    .unwrap();

    let indices_filled_root: IndexFilledNode<IndexLeaf> = match facts_filled_root.data {
        NodeData::Binary(binary_data) => {
            let children_indices = current_root_index.get_children_indices();
            convert_facts_to_index_layout_inner::<FactsLeaf, IndexLeaf>(
                facts_storage,
                index_layout_storage,
                children_indices[0],
                binary_data.left_data,
                trie_type,
            )
            .await;
            convert_facts_to_index_layout_inner::<FactsLeaf, IndexLeaf>(
                facts_storage,
                index_layout_storage,
                children_indices[1],
                binary_data.right_data,
                trie_type,
            )
            .await;
            IndexFilledNode(FilledNode {
                hash: current_root_hash,
                data: NodeData::Binary(BinaryData { left_data: (), right_data: () }),
            })
        }
        NodeData::Edge(edge_data) => {
            let bottom_index =
                NodeIndex::compute_bottom_index(current_root_index, &edge_data.path_to_bottom);
            convert_facts_to_index_layout_inner::<FactsLeaf, IndexLeaf>(
                facts_storage,
                index_layout_storage,
                bottom_index,
                edge_data.bottom_data,
                trie_type,
            )
            .await;
            IndexFilledNode(FilledNode {
                hash: current_root_hash,
                data: NodeData::Edge(EdgeData {
                    bottom_data: (),
                    path_to_bottom: edge_data.path_to_bottom,
                }),
            })
        }
        NodeData::Leaf(leaf) => IndexFilledNode(FilledNode {
            hash: current_root_hash,
            data: NodeData::Leaf(leaf.into()),
        }),
    };

    let index_db_key =
        create_db_key(IndexLeaf::get_static_prefix(trie_type), &current_root_index.0.to_be_bytes());
    index_layout_storage.set(index_db_key, indices_filled_root.serialize().unwrap()).await.unwrap();
}

impl TreeHashFunction<MockIndexLayoutLeaf> for TreeHashFunctionImpl {
    fn compute_leaf_hash(leaf_data: &MockIndexLayoutLeaf) -> HashOutput {
        HashOutput(leaf_data.0.0)
    }

    fn compute_node_hash(_node_data: &NodeData<MockIndexLayoutLeaf, HashOutput>) -> HashOutput {
        HashOutput(Felt::ZERO)
    }
}

#[apply(create_tree_cases)]
#[rstest]
#[tokio::test]
async fn test_create_tree_index_layout(
    #[case] mut case: CreateTreeCase,
    #[values(true, false)] compare_modified_leaves: bool,
) {
    let mut storage = convert_facts_db_to_index_db::<MockLeaf, MockIndexLayoutLeaf>(
        &mut case.storage,
        case.root_hash,
        Some(TrieType::StorageTrie(ContractAddress::from(0_u128))),
    )
    .await;

    let leaf_modifications: HashMap<NodeIndex, MockIndexLayoutLeaf> =
        case.leaf_modifications.into_iter().map(|(k, v)| (k, v.into())).collect();

    test_create_original_skeleton::<MockIndexLayoutLeaf, IndexNodeLayout>(
        &mut storage,
        &leaf_modifications,
        case.root_hash,
        &case.expected_skeleton_nodes,
        case.subtree_height,
        compare_modified_leaves,
    )
    .await;
}
