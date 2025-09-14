use std::collections::HashMap;

use ethnum::U256;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Pedersen;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::external_test_utils::{
    create_binary_entry,
    create_binary_entry_from_u128,
    create_edge_entry,
    create_edge_entry_from_u128,
    create_leaf_entry,
    create_leaf_patricia_key,
    AdditionHash,
};
use crate::patricia_merkle_tree::internal_test_utils::{small_tree_index_to_full, MockLeaf};
use crate::patricia_merkle_tree::node_data::inner_node::{
    to_preimage_map,
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    PathToBottom,
    Preimage,
    PreimageMap,
};
use crate::patricia_merkle_tree::traversal::{fetch_patricia_paths_inner, SubTree};
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
// Some cases uses addition hash and others (generated in python) use pedersen hash.
// For convenience, the leaves values are their NodeIndices.
/// This test simulates the main function
/// [`crate::patricia_merkle_tree::traversal::fetch_patricia_paths`], but with a tree of different
/// height. SubTree structure:
/// ```text
///           92
///       /        \
///     38           54
///    /   \       /    \
///   17   21     25    29
///  / \   / \   /  \   / \
/// 8   9 10 11 12  13 14  15
/// ```
/// Modified leaf indices: `[13]`
/// Expected nodes hashes: `[92, 54, 25]`
/// Siblings hashes (in preimage of nodes): `[38, 29, 12]`
#[case::binary_tree_one_leaf(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(10, 11),
        create_binary_entry_from_u128::<AdditionHash>(12, 13),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(17, 21),
        create_binary_entry_from_u128::<AdditionHash>(25, 29),
        create_binary_entry_from_u128::<AdditionHash>(38, 54),
        create_leaf_entry::<MockLeaf>(12),
        create_leaf_entry::<MockLeaf>(13),
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![13],
    SubTreeHeight::new(3),
    to_preimage_map(HashMap::from([
        (92, vec![38, 54]),
        (54, vec![25, 29]),
        (25, vec![12, 13]),
    ])),
)]
/// Modified leaf indices: `[12, 13]`
/// Expected nodes hashes: `[92, 54, 25]`
/// Siblings hashes (in preimage of nodes): `[38, 29]`
#[case::binary_tree_two_siblings(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(10, 11),
        create_binary_entry_from_u128::<AdditionHash>(12, 13),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(17, 21),
        create_binary_entry_from_u128::<AdditionHash>(25, 29),
        create_binary_entry_from_u128::<AdditionHash>(38, 54),
        create_leaf_entry::<MockLeaf>(12),
        create_leaf_entry::<MockLeaf>(13),
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![12, 13],
    SubTreeHeight::new(3),
    to_preimage_map(HashMap::from([
        (92, vec![38, 54]),
        (54, vec![25, 29]),
        (25, vec![12, 13]),
    ])),
)]
/// Modified leaf indices: `[11, 14]`
/// Expected nodes hashes: `[92, 38, 54, 21, 29]`
/// Siblings hashes (in preimage of nodes): `[17, 25, 10, 15]`
#[case::binary_tree_two_leaves(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(10, 11),
        create_binary_entry_from_u128::<AdditionHash>(12, 13),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(17, 21),
        create_binary_entry_from_u128::<AdditionHash>(25, 29),
        create_binary_entry_from_u128::<AdditionHash>(38, 54),
        create_leaf_entry::<MockLeaf>(10),
        create_leaf_entry::<MockLeaf>(11),
        create_leaf_entry::<MockLeaf>(14),
        create_leaf_entry::<MockLeaf>(15),
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![11, 14],
    SubTreeHeight::new(3),
    to_preimage_map(HashMap::from([
        (92, vec![38, 54]),
        (38, vec![17, 21]),
        (54, vec![25, 29]),
        (21, vec![10, 11]),
        (29, vec![14, 15]),
    ])),
)]
/// Modified leaf indices: `[8, 11, 12, 14]`
/// Expected nodes hashes: `[92, 38, 54, 17, 21, 25, 29]`
/// Siblings hashes (in preimage of nodes): `[9, 10, 13, 15]`
#[case::binary_many_leaves(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(10, 11),
        create_binary_entry_from_u128::<AdditionHash>(12, 13),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(17, 21),
        create_binary_entry_from_u128::<AdditionHash>(25, 29),
        create_binary_entry_from_u128::<AdditionHash>(38, 54),
        create_leaf_entry::<MockLeaf>(8),
        create_leaf_entry::<MockLeaf>(9),
        create_leaf_entry::<MockLeaf>(10),
        create_leaf_entry::<MockLeaf>(11),
        create_leaf_entry::<MockLeaf>(12),
        create_leaf_entry::<MockLeaf>(13),
        create_leaf_entry::<MockLeaf>(14),
        create_leaf_entry::<MockLeaf>(15),
    ]),
    HashOutput(Felt::from(92_u128)),
    vec![8, 11, 12, 14],
    SubTreeHeight::new(3),
    to_preimage_map(HashMap::from([
        (92, vec![38, 54]),
        (38, vec![17, 21]),
        (54, vec![25, 29]),
        (17, vec![8, 9]),
        (21, vec![10, 11]),
        (25, vec![12, 13]),
        (29, vec![14, 15]),
    ])),
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
/// Modified leaf indices: `[13]`
/// Expected nodes hashes: `[62, 44, 15]`
/// Siblings hashes (in preimage of nodes): `[18, 29]`
#[case::edge_one_leaf_edge(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(15, 29),
        create_binary_entry_from_u128::<AdditionHash>(18, 44),
        create_edge_entry_from_u128::<AdditionHash>(17, 0, 1),
        create_edge_entry_from_u128::<AdditionHash>(13, 1, 1),
        create_leaf_entry::<MockLeaf>(13),
    ]),
    HashOutput(Felt::from(62_u128)),
    vec![13],
    SubTreeHeight::new(3),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (62, vec![18, 44]),
        (44, vec![15, 29]),
        (15, vec![1, 1, 13]),
    ])),
)]
/// Modified leaf indices: `[14]`
/// Expected nodes hashes: `[62, 44, 29]`
/// Siblings hashes (in preimage of nodes): `[18, 15 (inner node), 15 (leaf)]`
#[case::edge_one_leaf_binary(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(15, 29),
        create_binary_entry_from_u128::<AdditionHash>(18, 44),
        create_edge_entry_from_u128::<AdditionHash>(17, 0, 1),
        create_edge_entry_from_u128::<AdditionHash>(13, 1, 1),
        create_leaf_entry::<MockLeaf>(14),
        create_leaf_entry::<MockLeaf>(15),
    ]),
    HashOutput(Felt::from(62_u128)),
    vec![14],
    SubTreeHeight::new(3),
    to_preimage_map(HashMap::from([
        (62, vec![18, 44]),
        (44, vec![15, 29]),
        (29, vec![14, 15]),
    ])),
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
/// Modified leaf indices: `[8]`
/// Expected nodes hashes: `[54, 10]`
/// Siblings hashes (in preimage of nodes): `[44]`
#[case::long_edge_one_leaf_edge(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(14, 15),
        create_binary_entry_from_u128::<AdditionHash>(15, 29),
        create_binary_entry_from_u128::<AdditionHash>(10, 44),
        create_edge_entry_from_u128::<AdditionHash>(8, 0, 2),
        create_edge_entry_from_u128::<AdditionHash>(13, 1, 1),
        create_leaf_entry::<MockLeaf>(8),
    ]),
    HashOutput(Felt::from(54_u128)),
    vec![8],
    SubTreeHeight::new(3),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (54, vec![10, 44]),
        (10, vec![2, 0, 8]),
    ])),
)]
/// SubTree structure:
/// ```text
/// 
///         38
///        /  \
///      18    20
///      /      \
///    17        *
///    / \        \
///   8   9       15
/// ```
/// Modified leaf indices: `[8]`
/// Expected nodes hashes: `[38, 18, 17]`
/// Siblings hashes (in preimage of nodes): `[20, 9]`
#[case::edge_and_binary(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(8, 9),
        create_edge_entry_from_u128::<AdditionHash>(17, 0, 1),
        create_edge_entry_from_u128::<AdditionHash>(15, 3, 2),
        create_binary_entry_from_u128::<AdditionHash>(18, 20),
        create_leaf_entry::<MockLeaf>(8),
        create_leaf_entry::<MockLeaf>(9),
    ]),
    HashOutput(Felt::from(38_u128)),
    vec![8],
    SubTreeHeight::new(3),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (38, vec![18, 20]),
        (18, vec![1, 0, 17]),
        (17, vec![8, 9]),
    ])),
)]
/// Test old tree with new leaves.
/// Old SubTree structure:
/// ```text
///          24
///         /
///         *
///          \
///          21
///          / \
///         10 11
/// ```
/// New SubTree structure:
/// ```text
///            52
///         /      \
///        38       14
///       / \       /
///     17   21    *
///     / \  / \  /
///    8  9 10 11 12
///   new new    new
/// ```
/// Expected nodes hashes: `[24]`
/// Siblings hashes (in preimage of nodes): `[21]`
#[case::should_return_empty_leaves(
    HashMap::from([
        create_binary_entry_from_u128::<AdditionHash>(10, 11),
        create_edge_entry_from_u128::<AdditionHash>(21, 1, 2),
        create_leaf_entry::<MockLeaf>(10),
        create_leaf_entry::<MockLeaf>(11),
    ]),
    HashOutput(Felt::from(24_u128)),
    vec![8, 9, 12],
    SubTreeHeight::new(3),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (24, vec![2, 1, 21]),
    ])),
)]
/// Python generated cases.
#[case::python_sparse_tree_1(
    HashMap::from([
    create_edge_entry_from_u128::<AdditionHash>(1471, 447, 9),
    create_edge_entry_from_u128::<AdditionHash>(1645, 109, 7),
    create_edge_entry_from_u128::<AdditionHash>(1757, 93, 7),
    create_edge_entry_from_u128::<AdditionHash>(1853, 61, 7),
    create_edge_entry_from_u128::<AdditionHash>(2000, 80, 7),
    create_binary_entry_from_u128::<AdditionHash>(1761, 1857),
    create_binary_entry_from_u128::<AdditionHash>(1921, 2087),
    create_binary_entry_from_u128::<AdditionHash>(3618, 4008),
    create_binary_entry_from_u128::<AdditionHash>(1927, 7626),
    create_leaf_entry::<MockLeaf>(1471),
    create_leaf_entry::<MockLeaf>(1645),
    create_leaf_entry::<MockLeaf>(1757),
    create_leaf_entry::<MockLeaf>(1853),
    create_leaf_entry::<MockLeaf>(2000),
 ]),
    HashOutput(Felt::from(9553_u128)),
    vec![1757, 1853],
    SubTreeHeight::new(10),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (9553, vec![1927, 7626]),
        (7626, vec![3618, 4008]),
        (3618, vec![1761, 1857]),
        (1857, vec![7, 93, 1757]),
        (4008, vec![1921, 2087]),
        (1921, vec![7, 61, 1853]),
    ])),
)]
#[case::python_sparse_tree_2(
    HashMap::from([
    create_edge_entry_from_u128::<AdditionHash>(1106, 82, 9),
    create_edge_entry_from_u128::<AdditionHash>(1554, 18, 8),
    create_edge_entry_from_u128::<AdditionHash>(2019, 99, 7),
    create_edge_entry_from_u128::<AdditionHash>(1812, 20, 6),
    create_edge_entry_from_u128::<AdditionHash>(1885, 29, 6),
    create_binary_entry_from_u128::<AdditionHash>(1838, 1920),
    create_binary_entry_from_u128::<AdditionHash>(3758, 2125),
    create_binary_entry_from_u128::<AdditionHash>(1580, 5883),
    create_binary_entry_from_u128::<AdditionHash>(1197, 7463),
    create_leaf_entry::<MockLeaf>(1812),
    create_leaf_entry::<MockLeaf>(1885),
    create_leaf_entry::<MockLeaf>(1106),
    create_leaf_entry::<MockLeaf>(1554),
    create_leaf_entry::<MockLeaf>(2019),
 ]),
    HashOutput(Felt::from(8660_u128)),
    vec![
        1554,
        1106,
    ],
    SubTreeHeight::new(10),
    // edge: [length, path, bottom]
    to_preimage_map(HashMap::from([
        (8660, vec![1197, 7463]),
        (1197, vec![9, 82, 1106]),
        (7463, vec![1580, 5883]),
        (1580, vec![8, 18, 1554]),
    ])),
)]
#[case::python_pedersen(
    HashMap::from([
    create_edge_entry::<Pedersen>(Felt::from_hex_unchecked("0x8"), 0, 1),
    create_edge_entry::<Pedersen>(Felt::from_hex_unchecked("0xb"), 1, 1),
    create_binary_entry::<Pedersen>(Felt::from_hex_unchecked("0xe"), Felt::from_hex_unchecked("0xf")),
    create_binary_entry::<Pedersen>(Felt::from_hex_unchecked("0x610eec7d913ae704e188746bc82767430e39e6f096188f4671712791c563a67"), Felt::from_hex_unchecked("0x25177dfc7f358239f3b7c4c1771ddcd7eaf74a1b2b2ac952f2c2dd52f5b860d")),
    create_edge_entry::<Pedersen>(Felt::from_hex_unchecked("0x54509ffe4af5d8674d8afbb218c8cb76554e12e96e9f0c97eb1c42b1e14edac"), 1, 1),
    create_binary_entry::<Pedersen>(Felt::from_hex_unchecked("0x111afbf8374248dc3a584bbd5f7c868f1dd76c3f17a326b5c77e692d736ece5"), Felt::from_hex_unchecked("0x20eec267afb39fcff7c97f9aa9e46ab73f61bf2e7db51c85a8f17cc313447fe")),
    create_leaf_entry::<MockLeaf>(14),
    create_leaf_entry::<MockLeaf>(15),
    create_leaf_entry::<MockLeaf>(11),
    create_leaf_entry::<MockLeaf>(8),
 ]),
    HashOutput(Felt::from_hex_unchecked("0xdd6634d8228819c6b4aec64cf4e5a39a420c77b75cdf08a85f73ae2f7afcc1")),
    vec![
        11,
        8,
    ],
    SubTreeHeight::new(3),
    PreimageMap::from([
        (HashOutput(Felt::from_hex_unchecked("0xdd6634d8228819c6b4aec64cf4e5a39a420c77b75cdf08a85f73ae2f7afcc1")),
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from_hex_unchecked("0x111afbf8374248dc3a584bbd5f7c868f1dd76c3f17a326b5c77e692d736ece5")),
            right_hash: HashOutput(Felt::from_hex_unchecked("0x20eec267afb39fcff7c97f9aa9e46ab73f61bf2e7db51c85a8f17cc313447fe")),
        })),
        (HashOutput(Felt::from_hex_unchecked("0x111afbf8374248dc3a584bbd5f7c868f1dd76c3f17a326b5c77e692d736ece5")),
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from_hex_unchecked("0x610eec7d913ae704e188746bc82767430e39e6f096188f4671712791c563a67")),
            right_hash: HashOutput(Felt::from_hex_unchecked("0x25177dfc7f358239f3b7c4c1771ddcd7eaf74a1b2b2ac952f2c2dd52f5b860d")),
        })),
        (HashOutput(Felt::from_hex_unchecked("0x610eec7d913ae704e188746bc82767430e39e6f096188f4671712791c563a67")),
        Preimage::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from_hex_unchecked("0x8")),
            path_to_bottom: PathToBottom::new(EdgePath(U256::from(0_u128)),
            EdgePathLength::new(1).unwrap())
            .unwrap()
        })),
        (HashOutput(Felt::from_hex_unchecked("0x25177dfc7f358239f3b7c4c1771ddcd7eaf74a1b2b2ac952f2c2dd52f5b860d")),
        Preimage::Edge(EdgeData {
            bottom_hash: HashOutput(Felt::from_hex_unchecked("0xb")),
            path_to_bottom: PathToBottom::new(EdgePath(U256::from(1_u128)),
            EdgePathLength::new(1).unwrap())
            .unwrap()
        })),
    ]),
)]
fn test_fetch_patricia_paths_inner(
    #[case] storage: MapStorage,
    #[case] root_hash: HashOutput,
    #[case] leaf_indices: Vec<u128>,
    #[case] height: SubTreeHeight,
    #[case] expected_nodes: PreimageMap,
) {
    let expected_fetched_leaves = leaf_indices
        .iter()
        .map(|&idx| {
            let leaf = if storage.contains_key(&create_leaf_patricia_key::<MockLeaf>(idx)) {
                MockLeaf(Felt::from(idx))
            } else {
                MockLeaf::default()
            };
            (small_tree_index_to_full(U256::from(idx), height), leaf)
        })
        .collect::<HashMap<_, _>>();

    let mut leaf_indices = leaf_indices
        .iter()
        .map(|&idx| small_tree_index_to_full(U256::from(idx), height))
        .collect::<Vec<_>>();
    let main_subtree = SubTree {
        sorted_leaf_indices: SortedLeafIndices::new(&mut leaf_indices),
        root_index: small_tree_index_to_full(U256::ONE, height),
        root_hash,
    };
    let mut nodes = HashMap::new();
    let mut fetched_leaves = HashMap::new();

    fetch_patricia_paths_inner::<MockLeaf>(
        &storage,
        vec![main_subtree],
        &mut nodes,
        Some(&mut fetched_leaves),
    )
    .unwrap();

    assert_eq!(nodes, expected_nodes);
    assert_eq!(fetched_leaves, expected_fetched_leaves);
}
