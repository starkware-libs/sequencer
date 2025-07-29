use std::collections::HashMap;

use ethnum::{uint, U256};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::tree::FilledTree;
use crate::patricia_merkle_tree::internal_test_utils::{
    as_fully_indexed,
    get_initial_updated_skeleton,
    small_tree_index_to_full,
    MockLeaf,
    MockTrie,
    OriginalSkeletonMockTrieConfig,
    TestTreeHashFunction,
};
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonNodeMap,
    OriginalSkeletonTreeImpl,
};
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::create_tree_helper::{
    get_path_to_lca,
    has_leaves_on_both_sides,
    TempSkeletonNode,
};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};

#[fixture]
fn initial_updated_skeleton(
    #[default(&[])] original_skeleton: &[(NodeIndex, OriginalSkeletonNode)],
    #[default(&[])] leaf_modifications: &[(NodeIndex, u8)],
) -> UpdatedSkeletonTreeImpl {
    get_initial_updated_skeleton(original_skeleton, leaf_modifications)
}

#[rstest]
#[case::small_tree_positive(
    3, 2, as_fully_indexed(subtree_height,
        vec![uint!("8"),uint!("10"),uint!("11")].into_iter()
    ),
    true)
    ]
#[case::small_tree_negative(3, 2, as_fully_indexed(
    subtree_height, vec![uint!("10"),uint!("11")].into_iter()), false)
    ]
#[case::large_tree_farthest_leaves(
    251,
    1,
    vec![NodeIndex::FIRST_LEAF, NodeIndex::MAX],
    true)]
#[case::large_tree_positive_consecutive_indices_of_different_sides(
    251,
    1,
    as_fully_indexed(subtree_height, vec![
        (U256::from(3u8) << 250) - U256::ONE, U256::from(3u8) << 250
    ].into_iter()
    ),
    true)]
#[case::large_tree_negative_one_shift_of_positive_case(
    251,
    1,
    as_fully_indexed(subtree_height, vec![
        U256::from(3u8) << 250, (U256::from(3u8) << 250) + U256::ONE
    ].into_iter()),
    false)]
fn test_has_leaves_on_both_sides(
    #[case] subtree_height: u8,
    #[case] root_index: u8,
    #[case] mut leaf_indices: Vec<NodeIndex>,
    #[case] expected: bool,
) {
    let height = SubTreeHeight(subtree_height);
    let root_index = small_tree_index_to_full(root_index.into(), height);
    assert_eq!(
        has_leaves_on_both_sides(&root_index, &SortedLeafIndices::new(&mut leaf_indices)),
        expected
    );
}

#[rstest]
#[case::first_leaf_not_descendant(3, 3, vec![NodeIndex::from(8), NodeIndex::from(12)])]
#[case::last_leaf_not_descendant(
    3,
    2,
    as_fully_indexed(3, vec![uint!("8"), uint!("12")].into_iter())
)]
#[should_panic(expected = "is not a descendant of the root")]
fn test_has_leaves_on_both_sides_assertions(
    #[case] subtree_height: u8,
    #[case] root_index: u8,
    #[case] mut leaf_indices: Vec<NodeIndex>,
) {
    let height = SubTreeHeight(subtree_height);
    let root_index = small_tree_index_to_full(root_index.into(), height);
    has_leaves_on_both_sides(&root_index, &SortedLeafIndices::new(&mut leaf_indices));
}

#[rstest]
#[case::small_tree_single_leaf(
    1,
    vec![U256::from(8_u8)],
    PathToBottom::new( U256::ZERO.into(), EdgePathLength::new(3).unwrap()).unwrap()
)]
#[case::small_tree_few_leaves(
    1,
    vec![
        U256::from(12_u8), U256::from(13_u8), U256::from(14_u8)
    ],
    PathToBottom::new(U256::ONE.into(), EdgePathLength::ONE).unwrap()
)]
#[case::small_tree_few_leaves2(
    1,
    vec![U256::from(12_u8),U256::from(13_u8)],
    PathToBottom::new(2_u128.into(), EdgePathLength::new(2).unwrap()).unwrap()
)]
#[case::large_tree_positive_consecutive_indices_of_different_sides(
    1,
    vec![(U256::from(3u8) << 250) - U256::ONE, U256::from(3u8) << 250],
    PathToBottom::new(U256::ZERO.into(), EdgePathLength::new(0).unwrap()).unwrap())]
#[case::large_tree_positive_consecutive_indices(
    3<<126,
    vec![U256::from(3u8) << 250, (U256::from(3u8) << 250)+ U256::ONE],
    PathToBottom::new(U256::ZERO.into(), EdgePathLength::new(123).unwrap()).unwrap())]
fn test_get_path_to_lca(
    #[case] root_index: u128,
    #[case] leaf_indices: Vec<U256>,
    #[case] expected: PathToBottom,
) {
    let root_index = NodeIndex::new(root_index.into());
    assert_eq!(
        get_path_to_lca(
            &root_index,
            &SortedLeafIndices::new(
                &mut leaf_indices
                    .iter()
                    .map(|index: &ethnum::U256| NodeIndex::new(*index))
                    .collect::<Vec<_>>()[..]
            )
        ),
        expected
    );
}

#[rstest]
#[case::two_deleted_leaves(
    &NodeIndex::from(1),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Empty,
    &[(NodeIndex::from(2),0), (NodeIndex::from(3),0)],
    TempSkeletonNode::Empty,
    &[]
)]
#[case::one_deleted_leaf(
    &NodeIndex::from(1),
    &TempSkeletonNode::Leaf,
    &TempSkeletonNode::Empty,
    &[(NodeIndex::from(2), 1), (NodeIndex::from(3), 0)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)
    ),
    &[]
)]
#[case::two_leaves(
    &NodeIndex::from(5),
    &TempSkeletonNode::Leaf,
    &TempSkeletonNode::Leaf,
    &[(NodeIndex::from(10),1), (NodeIndex::from(11),1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[]
)]
#[case::two_nodes(
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[
        (NodeIndex::from(10),UpdatedSkeletonNode::Binary),
        (NodeIndex::from(11), UpdatedSkeletonNode::Binary
    )]
)]
#[case::deleted_left_child(
    &NodeIndex::from(5),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[(NodeIndex::from(20), 0)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)),
    &[(NodeIndex::from(11),UpdatedSkeletonNode::Binary)]
)]
#[case::deleted_two_children(
    &NodeIndex::from(5),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Empty,
    &[(NodeIndex::from(20), 0), (NodeIndex::from(22), 0)],
    TempSkeletonNode::Empty,
    &[]
)]
#[case::left_edge_right_deleted(
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)),
    &TempSkeletonNode::Empty,
    &[(NodeIndex::from(22), 0)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::from("01"))),
    &[]
)]
fn test_node_from_binary_data(
    #[case] root_index: &NodeIndex,
    #[case] left: &TempSkeletonNode,
    #[case] right: &TempSkeletonNode,
    #[case] _leaf_modifications: &[(NodeIndex, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(&[], _leaf_modifications)] mut initial_updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut expected_skeleton_tree = initial_updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = initial_updated_skeleton.node_from_binary_data(root_index, left, right);
    assert_eq!(temp_node, expected_node);
    assert_eq!(initial_updated_skeleton.skeleton_tree, expected_skeleton_tree);
}

#[rstest]
#[case::to_empty(
    &PathToBottom::LEFT_CHILD,
    &NodeIndex::ROOT,
    &TempSkeletonNode::Empty,
    &[],
    TempSkeletonNode::Empty,
    &[],
)]
#[case::to_edge(
    &PathToBottom::from("00"),
    &NodeIndex::from(4),
    &TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::from("11"))
    ),
    &[],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::from("0011"))
    ),
    &[],
)]
#[case::to_unmodified_bottom(
    &PathToBottom::from("101"),
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::UnmodifiedSubTree(
        HashOutput::ZERO
    )),
   &[],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::from("101"))),
    &[],
)]
#[case::to_binary(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)
    ),
    &[(NodeIndex::from(7), UpdatedSkeletonNode::Binary)]
)]
#[case::to_non_empty_leaf(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Leaf,
    &[(NodeIndex::from(7), 1)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)
    ),
    &[]
)]
fn test_node_from_edge_data(
    #[case] path: &PathToBottom,
    #[case] bottom_index: &NodeIndex,
    #[case] bottom: &TempSkeletonNode,
    #[case] _leaf_modifications: &[(NodeIndex, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(&[], _leaf_modifications)] mut initial_updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut expected_skeleton_tree = initial_updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = initial_updated_skeleton.node_from_edge_data(path, bottom_index, bottom);
    assert_eq!(temp_node, expected_node);
    assert_eq!(initial_updated_skeleton.skeleton_tree, expected_skeleton_tree);
}

#[rstest]
#[case::one_leaf(
    &NodeIndex::ROOT,
    &[(NodeIndex::FIRST_LEAF, 1)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(251).as_str()))
    ),
    &[],
)]
// Note: the root is only finalized in the outer (create) function, so it doesn't appear in the
// skeleton created in the test.
#[case::leaves_on_both_sides(
    &NodeIndex::ROOT,
    &[(NodeIndex::FIRST_LEAF, 1), (NodeIndex::MAX, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[
        (NodeIndex::from(2),
        UpdatedSkeletonNode::Edge(PathToBottom::from("0".repeat(250).as_str()))),
        (NodeIndex::from(3),
        UpdatedSkeletonNode::Edge(PathToBottom::from("1".repeat(250).as_str())))],
)]
#[case::root_is_a_leaf(
    &NodeIndex::FIRST_LEAF,
    &[(NodeIndex::FIRST_LEAF, 1)],
    TempSkeletonNode::Leaf,
    &[]
)]
fn test_update_node_in_empty_tree(
    #[case] root_index: &NodeIndex,
    #[case] leaf_modifications: &[(NodeIndex, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(&[], leaf_modifications)] mut initial_updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut leaf_indices: Vec<NodeIndex> =
        leaf_modifications.iter().map(|(index, _)| *index).collect();
    let mut expected_skeleton_tree = initial_updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = initial_updated_skeleton
        .update_node_in_empty_tree(root_index, &SortedLeafIndices::new(&mut leaf_indices));
    assert_eq!(temp_node, expected_node);
    assert_eq!(initial_updated_skeleton.skeleton_tree, expected_skeleton_tree);
}

#[rstest]
#[case::modified_leaf(
    &NodeIndex::FIRST_LEAF,
    vec![
        (NodeIndex::FIRST_LEAF + 1,
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::ONE)))
    ],
    &[(NodeIndex::FIRST_LEAF, 1)],
    TempSkeletonNode::Leaf,
    &[],
)]
#[case::deleted_leaf(
    &NodeIndex::FIRST_LEAF,
    vec![
        (NodeIndex::FIRST_LEAF + 1,
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::ONE)))
    ],
    &[(NodeIndex::FIRST_LEAF, 0)],
    TempSkeletonNode::Empty,
    &[],
)]
#[case::orig_binary_with_modified_leaf(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF + 1,
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::ONE))),
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Binary)
    ],
    &[(NodeIndex::FIRST_LEAF, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
)]
#[case::orig_binary_with_deleted_leaf(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF + 1,
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::ONE))),
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Binary)
    ],
    &[(NodeIndex::FIRST_LEAF, 0)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)),
    &[],
)]
#[case::orig_binary_with_deleted_leaves(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![(NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Binary)],
    &[(NodeIndex::FIRST_LEAF, 0), (NodeIndex::FIRST_LEAF + 1, 0)],
    TempSkeletonNode::Empty,
    &[],
)]
#[case::orig_binary_with_binary_modified_children(
    &(NodeIndex::FIRST_LEAF >> 2),
    vec![
        (NodeIndex::FIRST_LEAF >> 2, OriginalSkeletonNode::Binary),
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Binary),
        ((NodeIndex::FIRST_LEAF >> 1) + 1,OriginalSkeletonNode::Binary)
    ],
    &[
        (NodeIndex::FIRST_LEAF, 1),
        (NodeIndex::FIRST_LEAF + 1, 1),
        (NodeIndex::FIRST_LEAF + 2, 1),
        (NodeIndex::FIRST_LEAF + 3, 1)
    ],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[
        (NodeIndex::FIRST_LEAF >> 1, UpdatedSkeletonNode::Binary),
        ((NodeIndex::FIRST_LEAF >> 1) + 1, UpdatedSkeletonNode::Binary)
    ],
)]
// The following cases test the `update_edge_node` function as well.
#[case::orig_edge_with_deleted_bottom(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
    ],
    &[(NodeIndex::FIRST_LEAF, 0)],
    TempSkeletonNode::Empty,
    &[],
)]
#[case::orig_edge_with_modified_bottom(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
    ],
    &[(NodeIndex::FIRST_LEAF, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
    &[],
)]
#[case::orig_edge_with_two_modified_leaves(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![(NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD))],
    &[(NodeIndex::FIRST_LEAF, 1), (NodeIndex::FIRST_LEAF + 1, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[
        (NodeIndex::FIRST_LEAF, UpdatedSkeletonNode::Leaf),
        (NodeIndex::FIRST_LEAF + 1, UpdatedSkeletonNode::Leaf)
    ],
)]
#[case::orig_edge_with_unmodified_bottom_and_added_leaf(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
        (NodeIndex::FIRST_LEAF, OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::ONE)))
    ],
    &[(NodeIndex::FIRST_LEAF + 1, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
)]
#[case::orig_edge_with_deleted_bottom_and_added_leaf(
    &(NodeIndex::FIRST_LEAF >> 1),
    vec![
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
    ],
    &[(NodeIndex::FIRST_LEAF, 0), (NodeIndex::FIRST_LEAF + 1, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::RIGHT_CHILD)),
    &[],
)]
#[case::orig_edge_with_modified_leaves_beneath_bottom(
    &(NodeIndex::FIRST_LEAF >> 2),
    vec![
        (NodeIndex::FIRST_LEAF >> 2, OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
        (NodeIndex::FIRST_LEAF >> 1, OriginalSkeletonNode::Binary),
    ],
    &[(NodeIndex::FIRST_LEAF, 1), (NodeIndex::FIRST_LEAF + 1, 1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge(PathToBottom::LEFT_CHILD)),
    &[(NodeIndex::FIRST_LEAF >> 1, UpdatedSkeletonNode::Binary)],
)]
fn test_update_node_in_nonempty_tree(
    #[case] root_index: &NodeIndex,
    #[case] original_skeleton: Vec<(NodeIndex, OriginalSkeletonNode)>,
    #[case] leaf_modifications: &[(NodeIndex, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(&original_skeleton, leaf_modifications)]
    mut initial_updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut original_skeleton: OriginalSkeletonNodeMap = original_skeleton.into_iter().collect();
    let mut leaf_indices: Vec<NodeIndex> =
        leaf_modifications.iter().map(|(index, _)| *index).collect();
    let mut expected_skeleton_tree = initial_updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = initial_updated_skeleton.update_node_in_nonempty_tree(
        root_index,
        &mut original_skeleton,
        &SortedLeafIndices::new(&mut leaf_indices),
    );
    assert_eq!(temp_node, expected_node);
    assert_eq!(initial_updated_skeleton.skeleton_tree, expected_skeleton_tree);
}

#[rstest]
#[case::empty_tree(HashOutput::ROOT_OF_EMPTY_TREE)]
#[case::non_empty_tree(HashOutput(Felt::from(77_u128)))]
#[tokio::test]
async fn test_update_non_modified_storage_tree(#[case] root_hash: HashOutput) {
    let empty_map = HashMap::new();
    let mut empty_storage = HashMap::new();
    let config = OriginalSkeletonMockTrieConfig::new(false);
    let mut original_skeleton_tree = OriginalSkeletonTreeImpl::create_impl::<MockLeaf>(
        &MapStorage { storage: &mut empty_storage },
        root_hash,
        SortedLeafIndices::new(&mut []),
        &config,
        &empty_map,
    )
    .unwrap();
    let updated =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton_tree, &HashMap::new()).unwrap();
    let filled = MockTrie::create_with_existing_leaves::<TestTreeHashFunction>(updated, empty_map)
        .await
        .unwrap();
    assert_eq!(root_hash, filled.get_root_hash());
}
