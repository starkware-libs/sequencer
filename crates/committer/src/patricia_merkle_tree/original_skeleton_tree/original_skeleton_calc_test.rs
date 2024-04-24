use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::original_skeleton_calc::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::types::{EdgePath, EdgePathLength, NodeIndex, PathToBottom};
use crate::storage::map_storage::MapStorage;
use crate::types::Felt;
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::collections::HashMap;

use crate::patricia_merkle_tree::types::TreeHeight;
use crate::storage::storage_trait::{StorageKey, StoragePrefix, StorageValue};

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
    create_binary_entry(8, 9),
    create_edge_entry(11, 1, 1),
    create_binary_entry(17, 13),
    create_edge_entry(15, 3, 2),
    create_binary_entry(30, 20)
    ]).into(),
    create_modifications(vec![(8, 4), (10, 3), (13, 2)]),
    HashOutput(Felt::from(50_u128)),
    HashMap::from([
        (NodeIndex::from(1), OriginalSkeletonNode::Binary),
        (NodeIndex::from(2), OriginalSkeletonNode::Binary),
        (NodeIndex::from(3), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::from(3_u128)),
                length: EdgePathLength(2),
            },
        }),
        (NodeIndex::from(4), OriginalSkeletonNode::Binary),
        (NodeIndex::from(5), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::from(1_u128)),
                length: EdgePathLength(1)
            }
        }),
        (NodeIndex::from(9), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(9_u128))
        )),
        (NodeIndex::from(15), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(15_u128))
        )),
        (NodeIndex::from(11), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(11_u128))
        )),
    ]),
    TreeHeight(3)
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
    create_binary_entry(10, 2),
    create_edge_entry(3, 1, 1),
    create_binary_entry(4, 7),
    create_edge_entry(12, 0, 1),
    create_binary_entry(5, 11),
    create_binary_entry(13, 16),
    ]).into(),
    create_modifications(vec![(8, 5), (11, 1), (13, 3)]),
    HashOutput(Felt::from(29_u128)),
    HashMap::from([
        (NodeIndex::from(1), OriginalSkeletonNode::Binary),
        (NodeIndex::from(2), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::ZERO),
                length: EdgePathLength(1)
            }
        }),
        (NodeIndex::from(3), OriginalSkeletonNode::Binary),
        (NodeIndex::from(4), OriginalSkeletonNode::Binary),
        (NodeIndex::from(6), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::from(1_u128)),
                length: EdgePathLength(1)
            }
        }),
        (NodeIndex::from(7), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(11_u128))
        )),
        (NodeIndex::from(9), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(2_u128))
        ))
    ]),
    TreeHeight(3)
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
///                         ES       B
///                        /      /     \
///                       *      E       B
///                      /        \     /  \
///                     24         *   E    B
///                                 \  /     \
///                                 20 5     40
///
#[case::tree_of_height_4_with_long_edge(
    HashMap::from([
    create_binary_entry(11, 13),
    create_edge_entry(5, 0, 1),
    create_binary_entry(19, 40),
    create_edge_entry(20, 3, 2),
    create_binary_entry(6, 59),
    create_edge_entry(24, 0, 2),
    create_binary_entry(25, 65),
    create_binary_entry(26, 90)
    ]).into(),
    create_modifications(vec![(18, 5), (25, 1), (29, 15), (30, 3)]),
    HashOutput(Felt::from(116_u128)),
    HashMap::from([
        (NodeIndex::from(1), OriginalSkeletonNode::Binary),
        (NodeIndex::from(2), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::ZERO),
                length: EdgePathLength(2)
            }
        }),
        (NodeIndex::from(3), OriginalSkeletonNode::Binary),
        (NodeIndex::from(6), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::from(3_u128)),
                length: EdgePathLength(2)
            }
        }),
        (NodeIndex::from(7), OriginalSkeletonNode::Binary),
        (NodeIndex::from(8), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(24_u128))
        )),
        (NodeIndex::from(14), OriginalSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::ZERO),
                length: EdgePathLength(1)
            }
        }),
        (NodeIndex::from(15), OriginalSkeletonNode::Binary),
        (NodeIndex::from(27), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(20_u128))
        )),
        (NodeIndex::from(28), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(5_u128))
        )),
        (NodeIndex::from(31), OriginalSkeletonNode::LeafOrBinarySibling(
            HashOutput(Felt::from(40_u128))
        )),

    ]),
    TreeHeight(4)
)]
fn test_fetch_nodes(
    #[case] storage: MapStorage,
    #[case] leaf_modifications: HashMap<NodeIndex, LeafData>,
    #[case] root_hash: HashOutput,
    #[case] expected_nodes: HashMap<NodeIndex, OriginalSkeletonNode<LeafData>>,
    #[case] tree_height: TreeHeight,
) {
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    sorted_leaf_indices.sort();

    let skeleton_tree = OriginalSkeletonTreeImpl::create_tree(
        &storage,
        &sorted_leaf_indices,
        root_hash,
        tree_height,
    )
    .unwrap();

    assert_eq!(&skeleton_tree.nodes, &expected_nodes);
}

fn create_32_bytes_entry(simple_val: u8) -> Vec<u8> {
    let mut res = vec![0; 31];
    res.push(simple_val);
    res
}

fn create_patricia_key(val: u8) -> StorageKey {
    StorageKey(create_32_bytes_entry(val)).with_prefix(StoragePrefix::PatriciaNode)
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

fn create_modifications(modifications: Vec<(u128, u128)>) -> HashMap<NodeIndex, LeafData> {
    modifications
        .into_iter()
        .map(|(idx, val)| {
            (
                NodeIndex::from(idx),
                LeafData::StorageValue(Felt::from(val)),
            )
        })
        .collect()
}

fn create_binary_entry(left: u8, right: u8) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(left + right),
        create_binary_val(left, right),
    )
}

fn create_edge_entry(hash: u8, path: u8, length: u8) -> (StorageKey, StorageValue) {
    (
        create_patricia_key(hash + path + length),
        create_edge_val(hash, path, length),
    )
}
