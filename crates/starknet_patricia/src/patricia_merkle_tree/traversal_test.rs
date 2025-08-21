use std::collections::HashMap;

use ethnum::U256;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::external_test_utils::{create_binary_entry, create_edge_entry};
use crate::patricia_merkle_tree::internal_test_utils::{small_tree_index_to_full, MockLeaf};
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    PathToBottom,
    Preimage,
    PreimageMap,
};
use crate::patricia_merkle_tree::traversal::{fetch_witnesses_inner, SubTree};
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

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
///        ^       /
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
    let tree = SubTree { sorted_leaf_indices, root_index, root_hash: HashOutput(Felt::ONE) };

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
    assert_eq!(previously_empty_leaf_indices, expected_previously_empty_leaf_indices);
    assert_eq!(subtree, expected_subtree);
}

fn create_previously_empty_leaf_indices<'a>(
    tree_leaf_indices: &'a [NodeIndex],
    subtree_leaf_indices: &'a [NodeIndex],
) -> Vec<&'a NodeIndex> {
    tree_leaf_indices.iter().filter(|idx| !subtree_leaf_indices.contains(idx)).collect()
}

#[rstest]
// This test assumes for simplicity that hash is addition (i.e hash(a,b) = a + b).
// For convenience, the leaves values are their NodeIndices.
/// SubTree structure:
/// ```text
///           92
///       /        \
///     38          54
///    /  \       /    \
///   17  21     25    29
///  / \  / \   /  \   / \
/// 8  9 10 11 12  13 14  15
/// ```
///
/// Modified leaves indices: `[13]`  
/// Expected witnesses hashes: `[25, 29, 38]`
#[case::binary_tree_one_leaf(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(10, 11),
        create_binary_entry(12, 13),
        create_binary_entry(14, 15),
        create_binary_entry(17, 21),
        create_binary_entry(25, 29),
        create_binary_entry(38, 54)
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![NodeIndex::from(13)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(25)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(12)),
                right_hash: HashOutput(Felt::from(13)),
            })),
        (HashOutput(Felt::from(29)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(14)),
                right_hash: HashOutput(Felt::from(15))
            })),
        (HashOutput(Felt::from(38)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(17)),
                right_hash: HashOutput(Felt::from(21))
            })),
    ]),
)]
/// Modified leaves indices: `[12, 13]`
/// Expected witnesses hashes: `[29, 38]`
#[case::binary_tree_two_siblings(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(10, 11),
        create_binary_entry(12, 13),
        create_binary_entry(14, 15),
        create_binary_entry(17, 21),
        create_binary_entry(25, 29),
        create_binary_entry(38, 54)
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![NodeIndex::from(12), NodeIndex::from(13)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(29)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(14)),
                right_hash: HashOutput(Felt::from(15))
            })),
        (HashOutput(Felt::from(38)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(17)),
                right_hash: HashOutput(Felt::from(21))
            })),
    ]),
)]
/// Modified leaves indices: `[11, 14]`
/// Expected witnesses hashes: `[17, 21, 25, 29]`
#[case::binary_tree_two_leaves(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(10, 11),
        create_binary_entry(12, 13),
        create_binary_entry(14, 15),
        create_binary_entry(17, 21),
        create_binary_entry(25, 29),
        create_binary_entry(38, 54)
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![NodeIndex::from(11), NodeIndex::from(14)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(17)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(8)),
                right_hash: HashOutput(Felt::from(9)),
            })),
        (HashOutput(Felt::from(21)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(10)),
                right_hash: HashOutput(Felt::from(11))
            })),
        (HashOutput(Felt::from(25)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(12)),
                right_hash: HashOutput(Felt::from(13)),
            })),
        (HashOutput(Felt::from(29)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(14)),
                right_hash: HashOutput(Felt::from(15))
            })),
    ]),
)]
/// Modified leaves indices: `[8, 11, 12, 14]`
/// Expected witnesses hashes: `[17, 21, 25, 29]`
#[case::binary_many_leaves(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(10, 11),
        create_binary_entry(12, 13),
        create_binary_entry(14, 15),
        create_binary_entry(17, 21),
        create_binary_entry(25, 29),
        create_binary_entry(38, 54)
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![NodeIndex::from(8), NodeIndex::from(11), NodeIndex::from(12), NodeIndex::from(14)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(17)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(8)),
                right_hash: HashOutput(Felt::from(9)),
            })),
        (HashOutput(Felt::from(21)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(10)),
                right_hash: HashOutput(Felt::from(11))
            })),
        (HashOutput(Felt::from(25)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(12)),
                right_hash: HashOutput(Felt::from(13)),
            })),
        (HashOutput(Felt::from(29)),
            Preimage::Binary(BinaryData {
                left_hash: HashOutput(Felt::from(14)),
                right_hash: HashOutput(Felt::from(15))
            })),
    ]),
)]
/// SubTree structure:
/// ```text
///          62
///        /    \
///      18      44
///     /      /    \
///    17     15    29
///   / \      \    / \
///  8   9     13  14  15
/// ```
/// Modified leaves indices: `[13]`
/// Expected witnesses hashes: `[29, 18]`
#[case::edge_one_leaf_edge(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(14, 15),
        create_binary_entry(15, 29),
        create_binary_entry(18, 44),
        create_edge_entry(17, 0, 1),
        create_edge_entry(13, 1, 1),
    ]),
    HashOutput(Felt::from(62_u128)),
    vec![NodeIndex::from(13)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(29)),
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(14)),
            right_hash: HashOutput(Felt::from(15)),
        })),
        (HashOutput(Felt::from(18)),
        Preimage::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from(17)),
            path_to_bottom: PathToBottom::new(EdgePath(U256::ZERO),
            EdgePathLength::new(1).unwrap())
            .unwrap()
        })),
    ]),
)]
/// Modified leaves indices: `[14]`
/// Expected witnesses hashes: `[15, 29, 18]`
#[case::edge_one_leaf_binary(
    HashMap::from([
        create_binary_entry(8, 9),
        create_binary_entry(14, 15),
        create_binary_entry(15, 29),
        create_binary_entry(18, 44),
        create_edge_entry(17, 0, 1),
        create_edge_entry(13, 1, 1),
    ]),
    HashOutput(Felt::from(62_u128)),
    vec![NodeIndex::from(14)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(15)),
        Preimage::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from(13)),
            path_to_bottom: PathToBottom::new(EdgePath(U256::ONE),
            EdgePathLength::new(1).unwrap())
            .unwrap()
        })),
        (HashOutput(Felt::from(29)),
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(14)),
            right_hash: HashOutput(Felt::from(15)),
        })),
        (HashOutput(Felt::from(18)),
        Preimage::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from(17)),
            path_to_bottom: PathToBottom::new(EdgePath(U256::ZERO),
            EdgePathLength::new(1).unwrap())
            .unwrap()
        })),
    ]),
)]
/// SubTree structure:
/// ```text
///         54
///        /  \
///      10    44
///     /     /   \
///    *     15    29
///   /        \   / \
///  8         13 14  15
/// ```
/// Modified leaves indices: `[8]`
/// Expected witnesses hashes: `[44]`
#[case::long_edge_one_leaf_edge(
    HashMap::from([
        create_binary_entry(14, 15),
        create_binary_entry(15, 29),
        create_binary_entry(10, 44),
        create_edge_entry(8, 0, 2),
        create_edge_entry(13, 1, 1),
    ]),
    HashOutput(Felt::from(54_u128)),
    vec![NodeIndex::from(8)],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from(44)),
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(15)),
            right_hash: HashOutput(Felt::from(29)),
        })),
    ]),
)]
fn test_fetch_witnesses_inner(
    #[case] storage: MapStorage,
    #[case] root_hash: HashOutput,
    #[case] leaf_indices: Vec<NodeIndex>,
    #[case] height: SubTreeHeight,
    #[case] expected_witnesses: PreimageMap,
) {
    let mut leaf_indices =
        leaf_indices.iter().map(|idx| small_tree_index_to_full(idx.0, height)).collect::<Vec<_>>();
    let main_subtree = SubTree {
        sorted_leaf_indices: SortedLeafIndices::new(&mut leaf_indices),
        root_index: small_tree_index_to_full(U256::ONE, height),
        root_hash,
    };
    let mut witnesses = HashMap::new();
    fetch_witnesses_inner::<MockLeaf>(&storage, vec![main_subtree], &mut witnesses).unwrap();
    assert_eq!(witnesses, expected_witnesses);
}
