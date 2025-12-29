use async_recursion::async_recursion;
use starknet_api::core::ContractAddress;
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::{FactDbFilledNode, FilledNode};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::FactNodeDeserializationContext;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{create_db_key, DbHashMap, DbValue, Storage};

use crate::block_committer::input::{try_node_index_into_contract_address, StarknetStorageValue};
use crate::db::facts_db::types::FactsSubTree;
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
    INDEX_LAYOUT_DB_KEY_SEPARATOR,
};
use crate::db::index_db::types::{EmptyNodeData, IndexFilledNode};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Converts a Facts-layout filled forest to Index-layout.
///
/// If skip_missing is true, missing nodes are silently skipped. Used when the original facts DB
/// only contains the tree subset that was necessary for tests. Use false when the input is known to
/// contain a full tree.
pub async fn convert_facts_forest_db_to_index_db(
    storage: &mut MapStorage,
    roots: StateRoots,
    skip_missing: bool,
) -> MapStorage {
    convert_forest(storage, roots, skip_missing).await
}

/// Converts a single Facts-layout trie to Index-layout.
/// Expects all nodes to exist (panics if a node is missing).
pub async fn convert_facts_db_to_index_db<FactsLeaf, IndexLeaf, KeyContext>(
    storage: &mut MapStorage,
    root_hash: HashOutput,
    key_context: &KeyContext,
    current_leaves: &mut Option<Vec<(NodeIndex, FactsLeaf)>>,
) -> MapStorage
where
    FactsLeaf: Leaf + Into<IndexLeaf> + HasStaticPrefix<KeyContext = KeyContext>,
    IndexLeaf: Leaf + HasStaticPrefix<KeyContext = KeyContext>,
    TreeHashFunctionImpl: TreeHashFunction<IndexLeaf>,
    KeyContext: Sync,
{
    convert_single_trie(storage, root_hash, key_context, current_leaves, false).await
}

/// Converts all tries in a forest from Facts-layout to Index-layout.
///
/// If skip_missing is true, missing nodes are silently skipped. Used when the original facts DB
/// only contains the tree subset that was necessary for tests. Use false when the input is known to
/// contain a full tree.
async fn convert_forest(
    storage: &mut MapStorage,
    roots: StateRoots,
    skip_missing: bool,
) -> MapStorage {
    let mut contract_leaves: Option<Vec<(NodeIndex, ContractState)>> = Some(Vec::new());
    let mut index_storage =
        convert_single_trie::<ContractState, IndexLayoutContractState, EmptyKeyContext>(
            storage,
            roots.contracts_trie_root_hash,
            &EmptyKeyContext,
            &mut contract_leaves,
            skip_missing,
        )
        .await
        .0;

    for (index, contract_leaf) in contract_leaves.unwrap() {
        let contract_address = try_node_index_into_contract_address(&index).unwrap();
        let storage_root = contract_leaf.storage_root_hash;
        let contract_storage_entries =
            convert_single_trie::<
                StarknetStorageValue,
                IndexLayoutStarknetStorageValue,
                ContractAddress,
            >(storage, storage_root, &contract_address, &mut None, skip_missing)
            .await;
        index_storage.extend(contract_storage_entries.0);
    }

    let classes_storage =
        convert_single_trie::<CompiledClassHash, IndexLayoutCompiledClassHash, EmptyKeyContext>(
            storage,
            roots.classes_trie_root_hash,
            &EmptyKeyContext,
            &mut None,
            skip_missing,
        )
        .await;
    index_storage.extend(classes_storage.0);

    MapStorage(index_storage)
}

/// Converts a single trie from Facts-layout to Index-layout.
async fn convert_single_trie<FactsLeaf, IndexLeaf, KeyContext>(
    storage: &mut MapStorage,
    root_hash: HashOutput,
    key_context: &KeyContext,
    current_leaves: &mut Option<Vec<(NodeIndex, FactsLeaf)>>,
    skip_missing: bool,
) -> MapStorage
where
    FactsLeaf: Leaf + Into<IndexLeaf> + HasStaticPrefix<KeyContext = KeyContext>,
    IndexLeaf: Leaf + HasStaticPrefix<KeyContext = KeyContext>,
    TreeHashFunctionImpl: TreeHashFunction<IndexLeaf>,
    KeyContext: Sync,
{
    let mut index_layout_storage = MapStorage(DbHashMap::new());
    let facts_sub_tree =
        FactsSubTree::create(SortedLeafIndices::default(), NodeIndex::ROOT, root_hash);

    traverse_and_convert::<FactsLeaf, IndexLeaf, KeyContext>(
        storage,
        &mut index_layout_storage,
        facts_sub_tree,
        key_context,
        current_leaves,
        skip_missing,
    )
    .await;
    index_layout_storage
}

/// Recursively traverses a Facts-layout trie and converts each node to Index-layout.
#[async_recursion]
async fn traverse_and_convert<FactsLeaf, IndexLeaf, KeyContext>(
    facts_storage: &mut MapStorage,
    index_layout_storage: &mut MapStorage,
    subtree: FactsSubTree<'async_recursion>,
    key_context: &KeyContext,
    current_leaves: &mut Option<Vec<(NodeIndex, FactsLeaf)>>,
    skip_missing: bool,
) where
    FactsLeaf: Leaf + Into<IndexLeaf> + HasStaticPrefix<KeyContext = KeyContext>,
    IndexLeaf: Leaf + HasStaticPrefix<KeyContext = KeyContext>,
    TreeHashFunctionImpl: TreeHashFunction<IndexLeaf>,
    KeyContext: Sync,
{
    if subtree.root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return;
    }

    let facts_db_key = subtree.get_root_db_key::<FactsLeaf>(key_context);

    // Try to get the node from storage.
    let filled_node_raw: Option<DbValue> = facts_storage.get(&facts_db_key).await.unwrap();

    // Handle missing nodes based on the skip_missing flag.
    let Some(filled_node_raw) = filled_node_raw else {
        if skip_missing {
            return;
        } else {
            panic!(
                "Node not found in storage: index={:?}, hash={:?}. If converting a filled forest, \
                 use convert_facts_filled_forest_to_index.",
                subtree.root_index, subtree.root_hash
            );
        }
    };

    let facts_filled_node = FactDbFilledNode::<FactsLeaf>::deserialize(
        &filled_node_raw,
        &FactNodeDeserializationContext {
            node_hash: subtree.root_hash,
            is_leaf: subtree.is_leaf(),
        },
    )
    .unwrap();

    let index_filled_node: IndexFilledNode<IndexLeaf> = match facts_filled_node.data {
        NodeData::Binary(binary_data) => {
            let (left_subtree, right_subtree) =
                subtree.get_children_subtrees(binary_data.left_data, binary_data.right_data);
            traverse_and_convert::<FactsLeaf, IndexLeaf, KeyContext>(
                facts_storage,
                index_layout_storage,
                left_subtree,
                key_context,
                current_leaves,
                skip_missing,
            )
            .await;
            traverse_and_convert::<FactsLeaf, IndexLeaf, KeyContext>(
                facts_storage,
                index_layout_storage,
                right_subtree,
                key_context,
                current_leaves,
                skip_missing,
            )
            .await;
            IndexFilledNode(FilledNode {
                hash: subtree.root_hash,
                data: NodeData::Binary(BinaryData {
                    left_data: EmptyNodeData,
                    right_data: EmptyNodeData,
                }),
            })
        }
        NodeData::Edge(edge_data) => {
            let (bottom_subtree, _) =
                subtree.get_bottom_subtree(&edge_data.path_to_bottom, edge_data.bottom_data);

            traverse_and_convert::<FactsLeaf, IndexLeaf, KeyContext>(
                facts_storage,
                index_layout_storage,
                bottom_subtree,
                key_context,
                current_leaves,
                skip_missing,
            )
            .await;
            IndexFilledNode(FilledNode {
                hash: subtree.root_hash,
                data: NodeData::Edge(EdgeData {
                    bottom_data: EmptyNodeData,
                    path_to_bottom: edge_data.path_to_bottom,
                }),
            })
        }
        NodeData::Leaf(leaf) => {
            if let Some(leaves) = current_leaves {
                leaves.push((subtree.root_index, leaf.clone()));
            }

            IndexFilledNode(FilledNode {
                hash: subtree.root_hash,
                data: NodeData::Leaf(leaf.into()),
            })
        }
    };

    let index_db_key = create_db_key(
        IndexLeaf::get_static_prefix(key_context),
        INDEX_LAYOUT_DB_KEY_SEPARATOR,
        &subtree.root_index.0.to_be_bytes(),
    );
    index_layout_storage.set(index_db_key, index_filled_node.serialize().unwrap()).await.unwrap();
}
