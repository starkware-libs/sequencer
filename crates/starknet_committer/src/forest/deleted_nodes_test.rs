use std::collections::{HashMap, HashSet};

use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use crate::block_committer::input::{
    contract_address_into_node_index,
    Input,
    ReaderConfig,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use crate::block_committer::measurements_util::NoMeasurements;
use crate::db::forest_trait::{ForestWriterWithMetadata, StorageInitializer};
use crate::db::index_db::db::{IndexDb, IndexDbReadContext};
use crate::forest::deleted_nodes::DeletedNodes;

const CONTRACT_ADDRESS: ContractAddress = ContractAddress(PatriciaKey::from_hex_unchecked("0x100"));

/// Commits a block with the given leaves and value, and writes the forest to storage.
/// Returns the deleted nodes.
async fn commit_block(
    leaves: Vec<u128>,
    value: u128,
    index_db: &mut IndexDb<MapStorage>,
) -> DeletedNodes {
    let storage_updates: HashMap<_, _> = leaves
        .iter()
        .map(|idx| (StarknetStorageKey::from(*idx), StarknetStorageValue(Felt::from(value))))
        .collect();

    let state_diff = StateDiff {
        storage_updates: HashMap::from([(CONTRACT_ADDRESS, storage_updates)]),
        ..Default::default()
    };
    let input = Input {
        state_diff,
        initial_read_context: IndexDbReadContext,
        config: ReaderConfig::new(true),
    };

    let (filled_forest, deleted_nodes) =
        CommitBlockImpl::commit_block(input, index_db, &mut NoMeasurements)
            .await
            .expect("Failed to commit block");

    // Write the forest to storage so the roots are persisted, including deleted nodes
    ForestWriterWithMetadata::write_with_metadata(
        index_db,
        &filled_forest,
        HashMap::new(), // Empty metadata
        &deleted_nodes,
    )
    .await
    .expect("Failed to write forest to storage");

    deleted_nodes
}

fn leaf_node_index(storage_key: u128) -> NodeIndex {
    (&StarknetStorageKey::from(storage_key)).into()
}

/// Tests the detection of deleted nodes.
/// Commits a block with the initial leaves and then a block with the deletion leaves.
/// Verifies that the deleted nodes match the expected set.
async fn verify_deleted_nodes(
    initial_leaves: Vec<u128>,
    leaves_to_delete: Vec<u128>,
    expected_deleted_nodes: HashSet<NodeIndex>,
) -> DeletedNodes {
    let mut index_db = IndexDb::new(MapStorage::default());

    // Commit block with original leaves (all non-zero)
    commit_block(initial_leaves, 100, &mut index_db).await;

    // Commit block with deletion leaves (all zero)
    let deleted_nodes = commit_block(leaves_to_delete, 0, &mut index_db).await;

    // Verify the deleted nodes match the expected set.
    let storage_trie_deleted_nodes = deleted_nodes.storage_tries.get(&CONTRACT_ADDRESS);
    assert_eq!(*storage_trie_deleted_nodes.unwrap_or(&HashSet::new()), expected_deleted_nodes);

    deleted_nodes
}

#[tokio::test]
async fn test_find_deleted_nodes_leaves_0_3_delete_3() {
    // Original tree:
    // B - E - 00 (0)
    //   \ E - 11 (3)

    // Updated tree after deleting leaf at index 3:
    // --- E - 00 (0)

    let leaf_0_node_index: NodeIndex = leaf_node_index(0u128);
    let leaf_3_node_index: NodeIndex = leaf_node_index(3u128);
    let leaf_3_parent = leaf_3_node_index >> 1;
    let leaf_0_parent = leaf_0_node_index >> 1;
    let common_ancestor = leaf_0_parent >> 1;

    let expected_deleted_nodes =
        HashSet::from([leaf_3_node_index, leaf_3_parent, leaf_0_parent, common_ancestor]);

    verify_deleted_nodes(vec![0, 3], vec![3], expected_deleted_nodes).await;
}

#[tokio::test]
async fn test_find_deleted_nodes_leaves_0_6_7_delete_0() {
    // Original tree:
    // B - E ----- 000 (0)
    //   \ E - B - 110 (6)
    //           \ 111 (7)
    //
    // Updated tree after deleting leaf at index 0:
    // ------- B - 110 (6)
    //           \ 111 (7)

    let leaf_0_node_index: NodeIndex = leaf_node_index(0u128);
    let leaf_0_parent = leaf_0_node_index >> 2;
    let leaf_0_parent_neighbor = leaf_0_parent + NodeIndex::from(1u128);
    let common_ancestor = leaf_0_parent >> 1;

    let expected_deleted_nodes =
        HashSet::from([leaf_0_node_index, leaf_0_parent, leaf_0_parent_neighbor, common_ancestor]);

    verify_deleted_nodes(vec![0, 6, 7], vec![0], expected_deleted_nodes).await;
}

#[tokio::test]
async fn test_find_deleted_nodes_leaves_0_1_7_delete_1() {
    // Original tree:
    // B - E - B - 000 (0)
    //   |       \ 001 (1)
    //   \ E ----- 111 (7)

    // Updated tree after deleting leaf at index 1:
    // B - E ----- 000 (0)
    //   \ E ----- 111 (7)

    let leaf_1_node_index: NodeIndex = leaf_node_index(1u128);
    let leaf_1_parent = leaf_1_node_index >> 1;

    let expected_deleted_nodes = HashSet::from([leaf_1_node_index, leaf_1_parent]);

    verify_deleted_nodes(vec![0, 1, 7], vec![1], expected_deleted_nodes).await;
}

#[tokio::test]
async fn test_find_deleted_nodes_leaves_0_1_2_3_delete_3() {
    // Original tree:
    // B - B - 00 (0)
    //   |   \ 01 (1)
    //   \ B - 10 (2)
    //       \ 11 (3)

    // Updated tree after deleting leaf at index 3:
    // B - B - 00 (0)
    //   |   \ 01 (1)
    //   \ E - 10 (2)

    // Convert storage keys to node indices
    let storage_key_3 = StarknetStorageKey::from(3u128);
    let leaf_3_node_index: NodeIndex = (&storage_key_3).into();

    // Calculate the expected deleted nodes:
    // When deleting leaf 3 from a tree with leaves 0, 1, 2, 3, only leaf 3 itself is deleted.
    // The other leaves (0, 1, 2) remain, so their parent nodes are still needed in the tree.
    let expected_deleted_nodes = HashSet::from([leaf_3_node_index]);

    verify_deleted_nodes(vec![0, 1, 2, 3], vec![3], expected_deleted_nodes).await;
}

#[tokio::test]
async fn test_delete_storage_trie() {
    let storage_key_0 = StarknetStorageKey::from(0u128);
    let leaf_0_node_index: NodeIndex = (&storage_key_0).into();

    let expected_deleted_nodes = HashSet::from([leaf_0_node_index, NodeIndex::ROOT]);

    let deleted_nodes = verify_deleted_nodes(vec![0], vec![0], expected_deleted_nodes).await;
    let expected_deleted_contracts_trie_nodes =
        HashSet::from([NodeIndex::ROOT, contract_address_into_node_index(&CONTRACT_ADDRESS)]);
    assert_eq!(deleted_nodes.contracts_trie, expected_deleted_contracts_trie_nodes);
}

#[tokio::test]
async fn test_unchanged_storage_trie() {
    let expected_deleted_nodes = HashSet::new();
    verify_deleted_nodes(vec![0, 3, 4], vec![], expected_deleted_nodes).await;
}
