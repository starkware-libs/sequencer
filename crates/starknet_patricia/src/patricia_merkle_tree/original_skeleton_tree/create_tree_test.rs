use std::collections::HashMap;

use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_patricia_storage::db_object::DBObject;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;

use super::OriginalSkeletonTreeImpl;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::external_test_utils::{
    create_binary_entry,
    create_binary_skeleton_node,
    create_edge_entry,
    create_edge_skeleton_node,
    create_expected_skeleton_nodes,
    create_root_edge_entry,
    create_unmodified_subtree_skeleton_node,
};
use crate::patricia_merkle_tree::internal_test_utils::{MockLeaf, OriginalSkeletonMockTrieConfig};
use crate::patricia_merkle_tree::node_data::leaf::LeafModifications;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

#[rstest]
// This test assumes for simplicity that hash is addition (i.e hash(a,b) = a + b).
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
    ]),
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
    ]),
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
///                       /      25      65
///                      /        \     /  \
///                     24         \   6   59
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
///                        /     E       B
///                       /       \     /  \
///                      24        \   E    B
///                                 \  /     \
///                                 20 5     40
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
    ]),
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
    let config = OriginalSkeletonMockTrieConfig::new(compare_modified_leaves);
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);
    let skeleton_tree = OriginalSkeletonTreeImpl::create::<MockLeaf>(
        &storage,
        root_hash,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
    .unwrap();
    assert_eq!(&skeleton_tree.nodes, &expected_skeleton_nodes);
}

pub(crate) fn create_mock_leaf_entry(val: u128) -> (DbKey, DbValue) {
    let leaf = MockLeaf(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

fn create_mock_leaf_modifications(
    leaf_modifications: Vec<(u128, u128)>,
) -> LeafModifications<MockLeaf> {
    leaf_modifications
        .into_iter()
        .map(|(idx, val)| (NodeIndex::from(idx), MockLeaf(Felt::from(val))))
        .collect()
}
