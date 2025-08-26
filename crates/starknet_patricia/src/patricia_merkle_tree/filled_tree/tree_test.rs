use std::collections::HashMap;

<<<<<<< HEAD
||||||| 01792faa8
use starknet_patricia_storage::map_storage::MapStorage;
=======
use starknet_patricia_storage::map_storage::BorrowedMapStorage;
>>>>>>> origin/main-v0.14.1
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use crate::patricia_merkle_tree::internal_test_utils::{
    MockLeaf,
    OriginalSkeletonMockTrieConfig,
    TestTreeHashFunction,
};
use crate::patricia_merkle_tree::node_data::errors::LeafError;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonNodeMap,
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};

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
    let root_hash = FilledTreeImpl::create_with_existing_leaves::<TestTreeHashFunction>(
        updated_skeleton_tree,
        modifications,
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
    let (updated_skeleton_tree, modifications) =
        get_small_tree_updated_skeleton_and_leaf_modifications();

    // Compute the hash values.
    let filled_tree = FilledTreeImpl::create_with_existing_leaves::<TestTreeHashFunction>(
        updated_skeleton_tree,
        modifications,
    )
    .await
    .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash();

    let (expected_filled_tree_map, expected_root_hash) =
        get_small_tree_expected_filled_tree_map_and_root_hash();
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
}

#[tokio::test(flavor = "multi_thread")]
/// Similar to `test_small_filled_tree`, except the tree is created via `FilledTree:create()`.
async fn test_small_filled_tree_create() {
    let (updated_skeleton_tree, modifications) =
        get_small_tree_updated_skeleton_and_leaf_modifications();
    let expected_leaf_index_to_leaf_output: HashMap<NodeIndex, String> =
        modifications.iter().map(|(index, leaf)| (*index, leaf.0.to_hex_string())).collect();
    let leaf_index_to_leaf_input: HashMap<NodeIndex, Felt> =
        modifications.into_iter().map(|(index, leaf)| (index, leaf.0)).collect();

    // Compute the hash values.
    let (filled_tree, leaf_index_to_leaf_output) = FilledTreeImpl::create::<TestTreeHashFunction>(
        updated_skeleton_tree,
        leaf_index_to_leaf_input,
    )
    .await
    .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash();

    let (expected_filled_tree_map, expected_root_hash) =
        get_small_tree_expected_filled_tree_map_and_root_hash();
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
    assert_eq!(
        leaf_index_to_leaf_output, expected_leaf_index_to_leaf_output,
        "Leaf output mismatch"
    );
}

#[tokio::test(flavor = "multi_thread")]
/// Test the edge case of creating a tree with no leaf modifications.
async fn test_empty_leaf_modifications() {
    let root_hash = HashOutput(Felt::ONE);
    let unmodified_updated_skeleton_tree_map =
        HashMap::from([(NodeIndex::ROOT, UpdatedSkeletonNode::UnmodifiedSubTree(root_hash))]);

    // Test `create_with_existing_leaves`.
    let filled_tree = FilledTreeImpl::create_with_existing_leaves::<TestTreeHashFunction>(
        UpdatedSkeletonTreeImpl { skeleton_tree: unmodified_updated_skeleton_tree_map.clone() },
        HashMap::new(),
    )
    .await
    .unwrap();
    assert_eq!(filled_tree.get_root_hash(), root_hash);
    assert!(filled_tree.get_all_nodes().is_empty());

    // Test `create`.
    let (filled_tree, leaf_index_to_leaf_output) = FilledTreeImpl::create::<TestTreeHashFunction>(
        UpdatedSkeletonTreeImpl { skeleton_tree: unmodified_updated_skeleton_tree_map },
        HashMap::new(),
    )
    .await
    .unwrap();
    assert_eq!(filled_tree.get_root_hash(), root_hash);
    assert!(filled_tree.get_all_nodes().is_empty());
    assert!(leaf_index_to_leaf_output.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
/// Test the edge case of creating a tree with an empty updated skeleton and non-empty leaf
/// modifications. This can only happen when the leaf modifications don't actually modify any nodes.
async fn test_empty_updated_skeleton() {
    let leaf_modifications = HashMap::from([(NodeIndex::FIRST_LEAF, Felt::ONE)]);

    // Test `create`.
    let (filled_tree, leaf_index_to_leaf_output) = FilledTreeImpl::create::<TestTreeHashFunction>(
        UpdatedSkeletonTreeImpl { skeleton_tree: HashMap::new() },
        leaf_modifications,
    )
    .await
    .unwrap();
    assert_eq!(filled_tree.get_root_hash(), HashOutput::ROOT_OF_EMPTY_TREE);
    assert!(filled_tree.get_all_nodes().is_empty());
    assert!(leaf_index_to_leaf_output.is_empty());

    // `create_with_existing_leaves` is tested in `test_delete_leaf_from_empty_tree`.
}

#[tokio::test(flavor = "multi_thread")]
/// Tests the case of a leaf computation error.
async fn test_leaf_computation_error() {
    let (first_leaf_index, second_leaf_index) = (NodeIndex(2_u32.into()), NodeIndex(3_u32.into()));
    let leaf_input_map =
        HashMap::from([(first_leaf_index, 1_u128.into()), (second_leaf_index, Felt::MAX)]);
    let skeleton_tree = HashMap::from([
        (NodeIndex::ROOT, UpdatedSkeletonNode::Binary),
        (first_leaf_index, UpdatedSkeletonNode::Leaf),
        (second_leaf_index, UpdatedSkeletonNode::Leaf),
    ]);

    let result = FilledTreeImpl::create::<TestTreeHashFunction>(
        UpdatedSkeletonTreeImpl { skeleton_tree },
        leaf_input_map,
    )
    .await;
    match result {
        Err(FilledTreeError::Leaf {
            leaf_error: LeafError::LeafComputationError(_),
            leaf_index,
        }) => {
            assert_eq!(leaf_index, second_leaf_index);
        }
        _ => panic!("Expected leaf computation error."),
    };
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
    let filled_tree = FilledTreeImpl::create_with_existing_leaves::<TestTreeHashFunction>(
        updated_skeleton_tree,
        modifications,
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
<<<<<<< HEAD
    let storage = HashMap::new();
||||||| 01792faa8
=======
    let mut storage = HashMap::new();
>>>>>>> origin/main-v0.14.1
    let mut original_skeleton_tree = OriginalSkeletonTreeImpl::create_impl(
<<<<<<< HEAD
        &storage,
||||||| 01792faa8
        &MapStorage { storage: HashMap::new() },
=======
        &BorrowedMapStorage { storage: &mut storage },
>>>>>>> origin/main-v0.14.1
        HashOutput::ROOT_OF_EMPTY_TREE,
        SortedLeafIndices::new(&mut indices),
        &OriginalSkeletonMockTrieConfig::new(false),
        &storage_modifications,
    )
    .unwrap();

    // Create an updated skeleton tree with a single leaf that is deleted.
    let skeleton_modifications = HashMap::from([(NodeIndex::FIRST_LEAF, SkeletonLeaf::Zero)]);

    let updated_skeleton_tree =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton_tree, &skeleton_modifications)
            .unwrap();

    let leaf_modifications = HashMap::from([(NodeIndex::FIRST_LEAF, MockLeaf(Felt::ZERO))]);
    // Compute the filled tree.
    let filled_tree = FilledTreeImpl::create_with_existing_leaves::<TestTreeHashFunction>(
        updated_skeleton_tree,
        leaf_modifications,
    )
    .await
    .unwrap();

    // The filled tree should be empty.
    let filled_tree_map = filled_tree.get_all_nodes();
    assert!(filled_tree_map.is_empty());
    let root_hash = filled_tree.get_root_hash();
    assert!(root_hash == HashOutput::ROOT_OF_EMPTY_TREE);
}

fn get_small_tree_updated_skeleton_and_leaf_modifications()
-> (UpdatedSkeletonTreeImpl, LeafModifications<MockLeaf>) {
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
    (updated_skeleton_tree, modifications)
}

fn get_small_tree_expected_filled_tree_map_and_root_hash()
-> (HashMap<NodeIndex, FilledNode<MockLeaf>>, HashOutput) {
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
    (expected_filled_tree_map, expected_root_hash)
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
