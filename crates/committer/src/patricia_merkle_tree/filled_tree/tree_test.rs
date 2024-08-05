use std::collections::HashMap;
use std::sync::Arc;

use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use crate::patricia_merkle_tree::internal_test_utils::{
    MockLeaf,
    OriginalSkeletonMockTrieConfig,
    TestTreeHashFunction,
};
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonNodeMap,
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};
use crate::storage::map_storage::MapStorage;

#[tokio::test(flavor = "multi_thread")]
/// This test is a sanity test for computing the root hash of the patricia merkle tree with a single
/// node that is a leaf with hash==1.
async fn test_filled_tree_sanity() {
    let mut skeleton_tree: UpdatedSkeletonNodeMap = HashMap::new();
    let new_filled_leaf = MockLeaf(Felt::ONE);
    let new_leaf_index = NodeIndex::ROOT;
    skeleton_tree.insert(new_leaf_index, UpdatedSkeletonNode::Leaf);
    let modifications = HashMap::from([(new_leaf_index, new_filled_leaf)]);
    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };
    let root_hash = FilledTreeImpl::create::<TestTreeHashFunction>(
        Arc::new(updated_skeleton_tree),
        Arc::new(modifications),
    )
    .await
    .unwrap()
    .get_root_hash();
    assert_eq!(root_hash, HashOutput(Felt::ONE), "Root hash mismatch");
}

// TODO(Aner, 11/4/25): Add test with large patricia merkle tree.
// TOOD(Aner, 11/4/25): Add test with different leaf types.

#[tokio::test(flavor = "multi_thread")]
/// This test is a small test for testing the root hash computation of the patricia merkle tree.
/// The tree structure & results were computed separately and tested for regression.
///                                i=1: binary
///                                /        \
///                        i=2: edge      i=3: edge
///                        l=1, p=0       l=4, p=15
///                      /                      \
///                 i=4: binary                  \
///                /           \                  \
///            i=8: edge    i=9: edge              \
///            l=2, p=3     l=2, p=0                \
///               \              /                   \
///                \            /                     \
///            i=35: leaf   i=36: leaf               i=63: leaf
///                  v=1          v=2                      v=3
async fn test_small_filled_tree() {
    // Set up the updated skeleton tree.
    let new_leaves = [(35, "0x1"), (36, "0x2"), (63, "0x3")];
    let nodes_in_skeleton_tree: Vec<(NodeIndex, UpdatedSkeletonNode)> = [
        create_binary_updated_skeleton_node_for_testing(1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(2, 0, 1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(3, 15, 4),
        create_binary_updated_skeleton_node_for_testing(4),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(8, 3, 2),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(9, 0, 2),
    ]
    .into_iter()
    .chain(
        new_leaves.iter().map(|(index, _)| create_leaf_updated_skeleton_node_for_testing(*index)),
    )
    .collect();
    let skeleton_tree: UpdatedSkeletonNodeMap = nodes_in_skeleton_tree.into_iter().collect();

    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };
    let modifications = new_leaves
        .iter()
        .map(|(index, value)| (NodeIndex::from(*index), MockLeaf(Felt::from_hex(value).unwrap())))
        .collect();

    // Compute the hash values.
    let filled_tree = FilledTreeImpl::create::<TestTreeHashFunction>(
        Arc::new(updated_skeleton_tree),
        Arc::new(modifications),
    )
    .await
    .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash();

    // The expected hash values were computed separately.
    let expected_root_hash = HashOutput(Felt::from_hex("0x21").unwrap());
    let expected_filled_tree_map = HashMap::from([
        create_mock_binary_entry_for_testing(1, "0x21", "0xb", "0x16"),
        create_mock_edge_entry_for_testing(2, "0xb", 0, 1, "0xa"),
        create_mock_edge_entry_for_testing(3, "0x16", 15, 4, "0x3"),
        create_mock_binary_entry_for_testing(4, "0xa", "0x6", "0x4"),
        create_mock_edge_entry_for_testing(8, "0x6", 3, 2, "0x1"),
        create_mock_edge_entry_for_testing(9, "0x4", 0, 2, "0x2"),
        create_mock_leaf_entry_for_testing(35, "0x1"),
        create_mock_leaf_entry_for_testing(36, "0x2"),
        create_mock_leaf_entry_for_testing(63, "0x3"),
    ]);
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
}

#[tokio::test(flavor = "multi_thread")]
/// This test is a small test for testing the root hash computation of the patricia merkle tree
/// with unmodified nodes. The tree structure & results are a partial of test_small_filled_tree.
///                   i=1: binary
///                   /        \
///            i=2: edge      i=3: unmodified
///            l=1, p=0       hash=0x3
///                /
///            i=4: binary
///          /           \
///      i=8: edge    i=9: unmodified
///      l=2, p=3     hash=0x4
///           \
///            \
///         i=35: leaf
///            v=1
async fn test_small_tree_with_unmodified_nodes() {
    // Set up the updated skeleton tree.
    let (new_leaf_index, new_leaf) = (35, "0x1");
    let nodes_in_skeleton_tree = [
        create_binary_updated_skeleton_node_for_testing(1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(2, 0, 1),
        create_unmodified_updated_skeleton_node_for_testing(3, "0x3"),
        create_binary_updated_skeleton_node_for_testing(4),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(8, 3, 2),
        create_unmodified_updated_skeleton_node_for_testing(9, "0x4"),
        create_leaf_updated_skeleton_node_for_testing(new_leaf_index),
    ];
    let skeleton_tree: UpdatedSkeletonNodeMap = nodes_in_skeleton_tree.into_iter().collect();

    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };
    let modifications = HashMap::from([(
        NodeIndex::from(new_leaf_index),
        MockLeaf(Felt::from_hex(new_leaf).unwrap()),
    )]);

    // Compute the hash values.
    let filled_tree = FilledTreeImpl::create::<TestTreeHashFunction>(
        Arc::new(updated_skeleton_tree),
        Arc::new(modifications),
    )
    .await
    .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash();

    // The expected hash values were computed separately. Note that the unmodified nodes are not
    // computed in the filled tree, but the hash values are directly used. The hashes of unmodified
    // nodes should not appear in the filled tree.
    let expected_root_hash = HashOutput(Felt::from_hex("0xe").unwrap());
    let expected_filled_tree_map = HashMap::from([
        create_mock_binary_entry_for_testing(1, "0xe", "0xb", "0x3"),
        create_mock_edge_entry_for_testing(2, "0b", 0, 1, "0xa"),
        create_mock_binary_entry_for_testing(4, "0xa", "0x6", "0x4"),
        create_mock_edge_entry_for_testing(8, "0x6", 3, 2, "0x1"),
        create_mock_leaf_entry_for_testing(35, "0x1"),
    ]);
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
}

#[tokio::test(flavor = "multi_thread")]
/// Test that deleting a leaf that does not exist in the tree succeeds.
async fn test_delete_leaf_from_empty_tree() {
    let storage_modifications: HashMap<NodeIndex, MockLeaf> =
        HashMap::from([(NodeIndex::FIRST_LEAF, MockLeaf(Felt::ZERO))]);

    let mut indices = [NodeIndex::FIRST_LEAF];
    // Create an empty original skeleton tree with a single leaf modified.
    let mut original_skeleton_tree = OriginalSkeletonTreeImpl::create_impl(
        &MapStorage { storage: HashMap::new() },
        HashOutput::ROOT_OF_EMPTY_TREE,
        SortedLeafIndices::new(&mut indices),
        &OriginalSkeletonMockTrieConfig::new(&storage_modifications, false),
    )
    .unwrap();

    // Create an updated skeleton tree with a single leaf that is deleted.
    let skeleton_modifications = HashMap::from([(NodeIndex::FIRST_LEAF, SkeletonLeaf::Zero)]);

    let updated_skeleton_tree =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton_tree, &skeleton_modifications)
            .unwrap();

    let leaf_modifications = HashMap::from([(NodeIndex::FIRST_LEAF, MockLeaf(Felt::ZERO))]);
    // Compute the filled tree.
    let filled_tree = FilledTreeImpl::create::<TestTreeHashFunction>(
        updated_skeleton_tree.into(),
        leaf_modifications.into(),
    )
    .await
    .unwrap();

    // The filled tree should be empty.
    let filled_tree_map = filled_tree.get_all_nodes();
    assert!(filled_tree_map.is_empty());
    let root_hash = filled_tree.get_root_hash();
    assert!(root_hash == HashOutput::ROOT_OF_EMPTY_TREE);
}

fn create_binary_updated_skeleton_node_for_testing(
    index: u128,
) -> (NodeIndex, UpdatedSkeletonNode) {
    (NodeIndex::from(index), UpdatedSkeletonNode::Binary)
}

fn create_path_to_bottom_edge_updated_skeleton_node_for_testing(
    index: u128,
    path: u128,
    length: u8,
) -> (NodeIndex, UpdatedSkeletonNode) {
    (
        NodeIndex::from(index),
        UpdatedSkeletonNode::Edge(
            PathToBottom::new(path.into(), EdgePathLength::new(length).unwrap()).unwrap(),
        ),
    )
}

fn create_unmodified_updated_skeleton_node_for_testing(
    index: u128,
    hash: &str,
) -> (NodeIndex, UpdatedSkeletonNode) {
    (
        NodeIndex::from(index),
        UpdatedSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::from_hex(hash).unwrap())),
    )
}

fn create_leaf_updated_skeleton_node_for_testing(index: u128) -> (NodeIndex, UpdatedSkeletonNode) {
    (NodeIndex::from(index), UpdatedSkeletonNode::Leaf)
}

fn create_mock_binary_entry_for_testing(
    index: u128,
    hash: &str,
    left_hash: &str,
    right_hash: &str,
) -> (NodeIndex, FilledNode<MockLeaf>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Binary(BinaryData {
                left_hash: HashOutput(Felt::from_hex(left_hash).unwrap()),
                right_hash: HashOutput(Felt::from_hex(right_hash).unwrap()),
            }),
        },
    )
}

fn create_mock_edge_entry_for_testing(
    index: u128,
    hash: &str,
    path: u128,
    length: u8,
    bottom_hash: &str,
) -> (NodeIndex, FilledNode<MockLeaf>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Edge(EdgeData {
                bottom_hash: HashOutput(Felt::from_hex(bottom_hash).unwrap()),
                path_to_bottom: PathToBottom::new(
                    path.into(),
                    EdgePathLength::new(length).unwrap(),
                )
                .unwrap(),
            }),
        },
    )
}

fn create_mock_leaf_entry_for_testing(
    index: u128,
    hash: &str,
) -> (NodeIndex, FilledNode<MockLeaf>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Leaf(MockLeaf(Felt::from_hex(hash).unwrap())),
        },
    )
}
