use super::OriginalSkeletonTreeImpl;
use crate::block_committer::input::StarknetStorageValue;
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::internal_test_utils::OriginalSkeletonMockTrieConfig;
use crate::patricia_merkle_tree::internal_test_utils::{small_tree_index_to_full, MockLeaf};
use crate::patricia_merkle_tree::node_data::inner_node::EdgePath;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, LeafModifications};
use crate::patricia_merkle_tree::original_skeleton_tree::create_tree::SubTree;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::SubTreeHeight;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use crate::storage::db_object::DBObject;
use crate::storage::map_storage::MapStorage;
use crate::storage::storage_trait::{create_db_key, StarknetPrefix, StorageKey, StorageValue};
use ethnum::U256;
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::collections::HashMap;

#[rstest]
// This test assumes for simplicity that hash is addition (i.e hash(a,b) = a + b).
///
///                 Old tree structure:
///
///                             50
///                           /   \
///                         30     20
///                        /  \     \
///                       17  13     *
///                      /  \   \     \
///                     8    9  11     15
///
///                   Modified leaves indices: [8, 10, 13]
///
///                   Expected skeleton:
///
///                             B
///                           /   \
///                          B     E
///                         / \     \
///                        B   E     *
///                       / \   \     \
///                      NZ  9  11    15
///
///

#[case::simple_tree_of_height_3(
    HashMap::from([
    create_root_edge_entry(50, SubTreeHeight::new(3)),
    create_binary_entry(8, 9),
    create_edge_entry(11, 1, 1),
    create_binary_entry(17, 13),
    create_edge_entry(15, 3, 2),
    create_binary_entry(30, 20),
    create_mock_leaf_entry(8),
    create_mock_leaf_entry(9),
    create_mock_leaf_entry(11),
    create_mock_leaf_entry(15)
    ]).into(),
    create_mock_leaf_modifications(vec![(8, 8), (10, 3), (13, 2)]),
    HashOutput(Felt::from(50_u128 + 248_u128)),
    create_expected_skeleton_nodes(
        vec![
            create_binary_skeleton_node(1),
            create_binary_skeleton_node(2),
            create_edge_skeleton_node(3, 3, 2),
            create_binary_skeleton_node(4),
            create_edge_skeleton_node(5, 1, 1),
            create_unmodified_subtree_skeleton_node(9, 9),
            create_unmodified_subtree_skeleton_node(15, 15),
            create_unmodified_subtree_skeleton_node(11, 11)
        ],
        3
    ),
    SubTreeHeight::new(3),
)]
///                 Old tree structure:
///
///                             29
///                           /    \
///                         13      16
///                        /      /    \
///                       12      5     11
///                      /  \      \    /  \
///                     10   2      3   4   7
///
///                   Modified leaves indices: [8, 11, 13]
///
///                   Expected skeleton:
///
///                             B
///                           /   \
///                         E      B
///                        /     /    \
///                       B      E     E
///                      /  \     \     \
///                     NZ   2     NZ    NZ
///

#[case::another_simple_tree_of_height_3(
    HashMap::from([
    create_root_edge_entry(29, SubTreeHeight::new(3)),
    create_binary_entry(10, 2),
    create_edge_entry(3, 1, 1),
    create_binary_entry(4, 7),
    create_edge_entry(12, 0, 1),
    create_binary_entry(5, 11),
    create_binary_entry(13, 16),
    create_mock_leaf_entry(10),
    create_mock_leaf_entry(2),
    create_mock_leaf_entry(3),
    create_mock_leaf_entry(4),
    create_mock_leaf_entry(7)
    ]).into(),
    create_mock_leaf_modifications(vec![(8, 5), (11, 1), (13, 3)]),
    HashOutput(Felt::from(29_u128 + 248_u128)),
    create_expected_skeleton_nodes(
        vec![
            create_binary_skeleton_node(1),
            create_edge_skeleton_node(2, 0, 1),
            create_binary_skeleton_node(3),
            create_binary_skeleton_node(4),
            create_edge_skeleton_node(6, 1, 1),
            create_unmodified_subtree_skeleton_node(7, 11),
            create_unmodified_subtree_skeleton_node(9, 2),
        ],
        3
    ),
    SubTreeHeight::new(3),
)]
///                  Old tree structure:
///
///                             116
///                           /     \
///                         26       90
///                        /      /     \
///                       *      25      65
///                      /        \     /  \
///                     24         *   6   59
///                    /  \         \  /  /  \
///                   11  13       20  5  19 40
///
///                   Modified leaves indices: [18, 25, 29, 30]
///
///                   Expected skeleton:
///
///                              B
///                           /     \
///                          E       B
///                         /     /     \
///                        *     E       B
///                       /       \     /  \
///                      24        *   E    B
///                                 \  /     \
///                                 20 5     40
///
#[case::tree_of_height_4_with_long_edge(
    HashMap::from([
    create_root_edge_entry(116, SubTreeHeight::new(4)),
    create_binary_entry(11, 13),
    create_edge_entry(5, 0, 1),
    create_binary_entry(19, 40),
    create_edge_entry(20, 3, 2),
    create_binary_entry(6, 59),
    create_edge_entry(24, 0, 2),
    create_binary_entry(25, 65),
    create_binary_entry(26, 90),
    create_mock_leaf_entry(11),
    create_mock_leaf_entry(13),
    create_mock_leaf_entry(20),
    create_mock_leaf_entry(5),
    create_mock_leaf_entry(19),
    create_mock_leaf_entry(40),
    ]).into(),
    create_mock_leaf_modifications(vec![(18, 5), (25, 1), (29, 15), (30, 19)]),
    HashOutput(Felt::from(116_u128 + 247_u128)),
    create_expected_skeleton_nodes(
        vec![
            create_binary_skeleton_node(1),
            create_edge_skeleton_node(2, 0, 2),
            create_binary_skeleton_node(3),
            create_edge_skeleton_node(6, 3, 2),
            create_binary_skeleton_node(7),
            create_unmodified_subtree_skeleton_node(8, 24),
            create_edge_skeleton_node(14, 0, 1),
            create_binary_skeleton_node(15),
            create_unmodified_subtree_skeleton_node(27, 20),
            create_unmodified_subtree_skeleton_node(28, 5),
            create_unmodified_subtree_skeleton_node(31, 40)
        ],
        4
    ),
    SubTreeHeight::new(4),
)]
fn test_create_tree(
    #[case] storage: MapStorage,
    #[case] leaf_modifications: LeafModifications<MockLeaf>,
    #[case] root_hash: HashOutput,
    #[case] expected_skeleton_nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    #[case] subtree_height: SubTreeHeight,
    #[values(true, false)] compare_modified_leaves: bool,
) {
    let leaf_modifications: LeafModifications<MockLeaf> = leaf_modifications
        .into_iter()
        .map(|(idx, leaf)| (NodeIndex::from_subtree_index(idx, subtree_height), leaf))
        .collect();
    let config = OriginalSkeletonMockTrieConfig::new(&leaf_modifications, compare_modified_leaves);
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);
    let skeleton_tree = OriginalSkeletonTreeImpl::create::<MockLeaf>(
        &storage,
        root_hash,
        sorted_leaf_indices,
        &config,
    )
    .unwrap();
    assert_eq!(&skeleton_tree.nodes, &expected_skeleton_nodes);
}

/// case::single_right_child
///     1
///      \
///       3
///
/// Bottom subtree:
///       3
///
/// case::single_left_child
///     1
///    /
///   2
///
/// Bottom subtree:
///       2
///
/// case::missing_nodes
///
///       1
///      /
///     *
///    /
///   4
///  /  \
/// 8   9
///
/// Bottom subtree:
///
///    4
///   /  \
///  8    9
///
/// case::long_left_path
///
///              1
///             /
///            *
///           /
///         ...
///         /
///
/// NodeIndex::FIRST_LEAF
///
/// Bottom subtree:
///
///  NodeIndex::FIRST_LEAF
///
/// case::long_right_path
///
///              1
///               \
///                *
///                 \
///                 ...
///
///                    \
///                    NodeIndex::MAX
///
/// Bottom subtree:
///
///    NodeIndex::MAX
///
/// case::should_delete_new_leaf
///
///           1
///          / \
///         2   new
///
/// Bottom subtree:
///
///      2
///
/// case::should_delete_new_leafs
///
///            1
///         /     \
///        *       *
///       / \     /
///      4   5   6
///     / \  / \  /
///    8  9 10 11 12
///   new new    new
///
/// Bottom subtree:
///
///      5
///     / \
///   11  10
///
#[rstest]
#[case::single_right_child(
        SubTreeHeight(1),
        &[U256::from(3_u128)],
        PathToBottom::new(EdgePath(U256::ONE), EdgePathLength::new(1).unwrap()).unwrap(),
        &[U256::from(3_u128)],
        U256::from(3_u128),
    )]
#[case::single_left_child(
    SubTreeHeight(1),
    &[U256::from(2_u128)],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(1).unwrap()).unwrap(),
    &[U256::from(2_u128)],
    U256::from(2_u128),
)]
#[case::missing_nodes(
    SubTreeHeight(3),
    &[U256::from(8_u128),U256::from(9_u128)],
    PathToBottom::new(EdgePath(U256::ZERO),EdgePathLength::new(2).unwrap()).unwrap(),
    &[U256::from(8_u128),U256::from(9_u128)],
    U256::from(4_u128),
)]
#[case::long_left_path(
    SubTreeHeight::ACTUAL_HEIGHT,
    &[NodeIndex::FIRST_LEAF.0],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0).unwrap()).unwrap(),
    &[NodeIndex::FIRST_LEAF.0],
    NodeIndex::FIRST_LEAF.0,
)]
#[case::long_right_path(
    SubTreeHeight::ACTUAL_HEIGHT,
    &[NodeIndex::MAX.0],
    PathToBottom::new(EdgePath(NodeIndex::MAX.0 >> 1), EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0).unwrap()).unwrap(),
    &[NodeIndex::MAX.0],
    NodeIndex::MAX.0,
    )]
#[case::should_delete_new_leaf(
    SubTreeHeight(1),
    &[U256::from(2_u128), U256::from(3_u128)],
    PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(1).unwrap()).unwrap(),
    &[U256::from(2_u128)],
    U256::from(2_u128),
)]
#[case::should_delete_new_leafs(
    SubTreeHeight(3),
    &[U256::from(8_u128), U256::from(9_u128), U256::from(10_u128), U256::from(11_u128), U256::from(12_u128)],
    PathToBottom::new(EdgePath(U256::ONE), EdgePathLength::new(2).unwrap()).unwrap(),
    &[U256::from(10_u128), U256::from(11_u128)],
    U256::from(5_u128),
)]
fn test_get_bottom_subtree(
    #[case] height: SubTreeHeight,
    #[case] sorted_leaf_indices: &[U256],
    #[case] path_to_bottom: PathToBottom,
    #[case] expected_sorted_leaf_indices: &[U256],
    #[case] expected_root_index: U256,
) {
    // Cast the input to the correct type for subtree.
    let root_index = small_tree_index_to_full(U256::ONE, height);

    let mut leaf_indices = sorted_leaf_indices
        .iter()
        .map(|&idx| small_tree_index_to_full(idx, height))
        .collect::<Vec<_>>();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut leaf_indices);
    // Cast the expected output to the correct type for subtree.
    let mut expected_leaf_indices = expected_sorted_leaf_indices
        .iter()
        .map(|&idx| small_tree_index_to_full(idx, height))
        .collect::<Vec<_>>();
    let expected_sorted_leaf_indices = SortedLeafIndices::new(&mut expected_leaf_indices);

    let expected_previously_empty_leaf_indices = create_previously_empty_leaf_indices(
        sorted_leaf_indices.get_indices(),
        expected_sorted_leaf_indices.get_indices(),
    );

    // Create the input Subtree.
    let tree = SubTree {
        sorted_leaf_indices,
        root_index,
        root_hash: HashOutput(Felt::ONE),
    };

    // Get the bottom subtree.
    let (subtree, previously_empty_leaf_indices) =
        tree.get_bottom_subtree(&path_to_bottom, HashOutput(Felt::TWO));

    let expected_root_index = small_tree_index_to_full(expected_root_index, height);

    // Create the expected subtree.
    let expected_subtree = SubTree {
        sorted_leaf_indices: expected_sorted_leaf_indices,
        root_index: expected_root_index,
        root_hash: HashOutput(Felt::TWO),
    };
    assert_eq!(
        previously_empty_leaf_indices,
        expected_previously_empty_leaf_indices
    );
    assert_eq!(subtree, expected_subtree);
}

pub(crate) fn create_32_bytes_entry(simple_val: u128) -> [u8; 32] {
    U256::from(simple_val).to_be_bytes()
}

pub(crate) fn create_mock_leaf_entry(val: u128) -> (StorageKey, StorageValue) {
    let leaf = MockLeaf(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

pub(crate) fn create_storage_leaf_entry(val: u128) -> (StorageKey, StorageValue) {
    let leaf = StarknetStorageValue(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

pub(crate) fn create_compiled_class_leaf_entry(val: u128) -> (StorageKey, StorageValue) {
    let leaf = CompiledClassHash(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

pub(crate) fn create_contract_state_leaf_entry(val: u128) -> (StorageKey, StorageValue) {
    let felt = Felt::from(val);
    let leaf = ContractState {
        nonce: Nonce(felt),
        storage_root_hash: HashOutput(felt),
        class_hash: ClassHash(felt),
    };
    (leaf.get_db_key(&felt.to_bytes_be()), leaf.serialize())
}

fn create_patricia_key(val: u128) -> StorageKey {
    create_db_key(
        StarknetPrefix::InnerNode.to_storage_prefix(),
        &U256::from(val).to_be_bytes(),
    )
}

fn create_binary_val(left: u128, right: u128) -> StorageValue {
    StorageValue(
        (create_32_bytes_entry(left)
            .into_iter()
            .chain(create_32_bytes_entry(right)))
        .collect(),
    )
}

fn create_edge_val(hash: u128, path: u128, length: u8) -> StorageValue {
    StorageValue(
        create_32_bytes_entry(hash)
            .into_iter()
            .chain(create_32_bytes_entry(path))
            .chain([length])
            .collect(),
    )
}

fn create_mock_leaf_modifications(
    leaf_modifications: Vec<(u128, u128)>,
) -> LeafModifications<MockLeaf> {
    leaf_modifications
        .into_iter()
        .map(|(idx, val)| (NodeIndex::from(idx), MockLeaf(Felt::from(val))))
        .collect()
}

pub(crate) fn create_binary_entry(left: u128, right: u128) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(left + right),
        create_binary_val(left, right),
    )
}

pub(crate) fn create_edge_entry(hash: u128, path: u128, length: u8) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(hash + path + u128::from(length)),
        create_edge_val(hash, path, length),
    )
}

pub(crate) fn create_expected_skeleton_nodes(
    nodes: Vec<(NodeIndex, OriginalSkeletonNode)>,
    height: u8,
) -> HashMap<NodeIndex, OriginalSkeletonNode> {
    let subtree_height = SubTreeHeight::new(height);
    nodes
        .into_iter()
        .map(|(node_index, node)| {
            (
                NodeIndex::from_subtree_index(node_index, subtree_height),
                node,
            )
        })
        .chain([(
            NodeIndex::ROOT,
            OriginalSkeletonNode::Edge(
                PathToBottom::new(0.into(), EdgePathLength::new(251 - height).unwrap()).unwrap(),
            ),
        )])
        .collect()
}

pub(crate) fn create_binary_skeleton_node(idx: u128) -> (NodeIndex, OriginalSkeletonNode) {
    (NodeIndex::from(idx), OriginalSkeletonNode::Binary)
}

pub(crate) fn create_edge_skeleton_node(
    idx: u128,
    path: u128,
    length: u8,
) -> (NodeIndex, OriginalSkeletonNode) {
    (
        NodeIndex::from(idx),
        OriginalSkeletonNode::Edge(
            PathToBottom::new(path.into(), EdgePathLength::new(length).unwrap()).unwrap(),
        ),
    )
}

pub(crate) fn create_unmodified_subtree_skeleton_node(
    idx: u128,
    hash_output: u128,
) -> (NodeIndex, OriginalSkeletonNode) {
    (
        NodeIndex::from(idx),
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::from(hash_output))),
    )
}

pub(crate) fn create_root_edge_entry(
    old_root: u128,
    subtree_height: SubTreeHeight,
) -> (StorageKey, StorageValue) {
    // Assumes path is 0.
    let length = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
    let new_root = old_root + u128::from(length);
    let key = create_db_key(
        StarknetPrefix::InnerNode.to_storage_prefix(),
        &Felt::from(new_root).to_bytes_be(),
    );
    let value = StorageValue(
        Felt::from(old_root)
            .to_bytes_be()
            .into_iter()
            .chain(Felt::from(0_u128).to_bytes_be())
            .chain([length])
            .collect(),
    );
    (key, value)
}

fn create_previously_empty_leaf_indices<'a>(
    tree_leaf_indices: &'a [NodeIndex],
    subtree_leaf_indices: &'a [NodeIndex],
) -> Vec<&'a NodeIndex> {
    tree_leaf_indices
        .iter()
        .filter(|idx| !subtree_leaf_indices.contains(idx))
        .collect()
}
