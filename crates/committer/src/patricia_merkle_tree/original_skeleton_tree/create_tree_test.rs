use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::LeafDataImpl;
use crate::patricia_merkle_tree::node_data::leaf::LeafModifications;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::map_storage::MapStorage;
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::collections::HashMap;

use crate::patricia_merkle_tree::types::SubTreeHeight;
use crate::storage::storage_trait::{create_db_key, StorageKey, StoragePrefix, StorageValue};

use super::OriginalSkeletonTreeImpl;

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
    create_root_edge_entry(50, SubTreeHeight::new(3)),
    create_binary_entry(8, 9),
    create_edge_entry(11, 1, 1),
    create_binary_entry(17, 13),
    create_edge_entry(15, 3, 2),
    create_binary_entry(30, 20)
    ]).into(),
    create_leaf_modifications(vec![(8, 4), (10, 3), (13, 2)]),
    HashOutput(Felt::from(50_u128 + 248_u128)),
    create_expected_skeleton(
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
    SubTreeHeight::new(3)
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
    ]).into(),
    create_leaf_modifications(vec![(8, 5), (11, 1), (13, 3)]),
    HashOutput(Felt::from(29_u128 + 248_u128)),
    create_expected_skeleton(
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
    SubTreeHeight::new(3)
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
    create_binary_entry(26, 90)
    ]).into(),
    create_leaf_modifications(vec![(18, 5), (25, 1), (29, 15), (30, 3)]),
    HashOutput(Felt::from(116_u128 + 247_u128)),
    create_expected_skeleton(
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
    SubTreeHeight::new(4)
)]
fn test_create_tree(
    #[case] storage: MapStorage,
    #[case] leaf_modifications: LeafModifications<LeafDataImpl>,
    #[case] root_hash: HashOutput,
    #[case] expected_skeleton: OriginalSkeletonTreeImpl,
    #[case] subtree_height: SubTreeHeight,
) {
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications
        .keys()
        .map(|idx| NodeIndex::from_subtree_index(*idx, subtree_height))
        .collect();

    sorted_leaf_indices.sort();

    let skeleton_tree =
        OriginalSkeletonTreeImpl::create(&storage, &sorted_leaf_indices, root_hash).unwrap();

    assert_eq!(&skeleton_tree.nodes, &expected_skeleton.nodes);
}

pub(crate) fn create_32_bytes_entry(simple_val: u8) -> Vec<u8> {
    let mut res = vec![0; 31];
    res.push(simple_val);
    res
}

fn create_patricia_key(val: u8) -> StorageKey {
    create_db_key(StoragePrefix::InnerNode, &create_32_bytes_entry(val))
}

fn create_binary_val(left: u8, right: u8) -> StorageValue {
    StorageValue(
        (create_32_bytes_entry(left)
            .into_iter()
            .chain(create_32_bytes_entry(right)))
        .collect(),
    )
}

fn create_edge_val(hash: u8, path: u8, length: u8) -> StorageValue {
    StorageValue(
        create_32_bytes_entry(hash)
            .into_iter()
            .chain(create_32_bytes_entry(path))
            .chain([length])
            .collect(),
    )
}

fn create_leaf_modifications(
    leaf_modifications: Vec<(u128, u128)>,
) -> LeafModifications<LeafDataImpl> {
    leaf_modifications
        .into_iter()
        .map(|(idx, val)| {
            (
                NodeIndex::from(idx),
                LeafDataImpl::StorageValue(Felt::from(val)),
            )
        })
        .collect()
}

pub(crate) fn create_binary_entry(left: u8, right: u8) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(left + right),
        create_binary_val(left, right),
    )
}

pub(crate) fn create_edge_entry(hash: u8, path: u8, length: u8) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(hash + path + length),
        create_edge_val(hash, path, length),
    )
}

pub(crate) fn create_expected_skeleton(
    nodes: Vec<(NodeIndex, OriginalSkeletonNode)>,
    height: u8,
) -> OriginalSkeletonTreeImpl {
    let subtree_height = SubTreeHeight::new(height);
    OriginalSkeletonTreeImpl {
        nodes: nodes
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
                    PathToBottom::new(0.into(), EdgePathLength::new(251 - height).unwrap())
                        .unwrap(),
                ),
            )])
            .collect(),
    }
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
    old_root: u8,
    subtree_height: SubTreeHeight,
) -> (StorageKey, StorageValue) {
    // Assumes path is 0.
    let length = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
    let new_root = u128::from(old_root) + u128::from(length);
    let key = create_db_key(
        StoragePrefix::InnerNode,
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
