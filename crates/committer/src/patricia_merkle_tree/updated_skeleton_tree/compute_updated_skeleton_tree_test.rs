use ethnum::U256;
use rstest::{fixture, rstest};

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgeData, EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::compute_updated_skeleton_tree::{
    get_path_to_lca, has_leaves_on_both_sides, TempSkeletonNode,
};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[fixture]
fn updated_skeleton(
    #[default(TreeHeight::MAX)] tree_height: TreeHeight,
    #[default(&[])] leaf_modifications: &[(u128, u8)],
) -> UpdatedSkeletonTreeImpl {
    UpdatedSkeletonTreeImpl {
        tree_height,
        skeleton_tree: leaf_modifications
            .iter()
            .filter(|(_, leaf_val)| *leaf_val != 0)
            .map(|(index, leaf_val)| {
                (
                    NodeIndex::from(*index),
                    UpdatedSkeletonNode::Leaf(SkeletonLeaf::from(*leaf_val)),
                )
            })
            .collect(),
    }
}

#[rstest]
#[case::small_tree_positive(
    3, 2, vec![NodeIndex::from(8),NodeIndex::from(10),NodeIndex::from(11)], true)]
#[case::small_tree_negative(3, 2, vec![NodeIndex::from(10),NodeIndex::from(11)], false)]
#[case::large_tree_farthest_leaves(
    251,
    1,
    vec![NodeIndex::ROOT << 251, NodeIndex::MAX],
    true)]
#[case::large_tree_positive_consecutive_indices_of_different_sides(
    251,
    1,
    vec![NodeIndex::new((U256::from(3u8) << 250) - U256::ONE), NodeIndex::new(U256::from(3u8) << 250)],
    true)]
#[case::large_tree_negative_one_shift_of_positive_case(
    251,
    1,
    vec![NodeIndex::new(U256::from(3u8) << 250), NodeIndex::new((U256::from(3u8) << 250)+ U256::ONE)],
    false)]
fn test_has_leaves_on_both_sides(
    #[case] _tree_height: u8,
    #[with(TreeHeight::new(_tree_height), &[])] updated_skeleton: UpdatedSkeletonTreeImpl,
    #[case] root_index: u8,
    #[case] leaf_indices: Vec<NodeIndex>,
    #[case] expected: bool,
) {
    let root_index = NodeIndex::new(root_index.into());
    assert_eq!(
        has_leaves_on_both_sides(&updated_skeleton.tree_height, &root_index, &leaf_indices),
        expected
    );
}

#[rstest]
#[case::first_leaf_not_descendant(3, 3, vec![NodeIndex::from(8), NodeIndex::from(12)])]
#[case::last_leaf_not_descendant(3, 2, vec![NodeIndex::from(8), NodeIndex::from(12)])]
#[should_panic(expected = "is not a descendant of the root")]
fn test_has_leaves_on_both_sides_assertions(
    #[case] _tree_height: u8,
    #[with(TreeHeight::new(_tree_height), &[])] updated_skeleton: UpdatedSkeletonTreeImpl,
    #[case] root_index: u8,
    #[case] leaf_indices: Vec<NodeIndex>,
) {
    let root_index = NodeIndex::new(root_index.into());
    has_leaves_on_both_sides(&updated_skeleton.tree_height, &root_index, &leaf_indices);
}

#[rstest]
#[case::small_tree_single_leaf(
    1, vec![U256::from(8_u8)], PathToBottom {path:U256::ZERO.into(), length:EdgePathLength(3)}
)]
#[case::small_tree_few_leaves(
    1,
    vec![
        U256::from(12_u8), U256::from(13_u8), U256::from(14_u8)
    ],
    PathToBottom {path:U256::ONE.into(), length:EdgePathLength(1)}
)]
#[case::small_tree_few_leaves2(
    1,
    vec![U256::from(12_u8),U256::from(13_u8)],
    PathToBottom {path:2_u128.into(), length:EdgePathLength(2)}
)]
#[case::large_tree_positive_consecutive_indices_of_different_sides(
    1,
    vec![(U256::from(3u8) << 250) - U256::ONE, U256::from(3u8) << 250],
    PathToBottom {path:U256::ZERO.into(), length:EdgePathLength(0)})]
#[case::large_tree_positive_consecutive_indices(
    3<<126,
    vec![U256::from(3u8) << 250, (U256::from(3u8) << 250)+ U256::ONE],
    PathToBottom {path:U256::ZERO.into(), length:EdgePathLength(123)})]
fn test_get_path_to_lca(
    #[case] root_index: u128,
    #[case] leaf_indices: Vec<U256>,
    #[case] expected: PathToBottom,
) {
    let root_index = NodeIndex::new(root_index.into());
    assert_eq!(
        get_path_to_lca(
            &root_index,
            &leaf_indices
                .iter()
                .map(|index: &ethnum::U256| NodeIndex::new(*index))
                .collect::<Vec<_>>()[..]
        ),
        expected
    );
}

#[rstest]
#[case::two_deleted_leaves(
    &NodeIndex::from(1),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Empty,
    &[(2,0), (3,0)],
    TempSkeletonNode::Empty,
    &[]
)]
#[case::one_deleted_leaf(
    &NodeIndex::from(1),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &TempSkeletonNode::Empty,
    &[(2, 1), (3, 0)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge {path_to_bottom: PathToBottom::LEFT_CHILD}
    ),
    &[]
)]
#[case::two_leaves(
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &[(10,1), (11,1)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[]
)]
#[case::two_nodes(
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
    TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[(NodeIndex::from(10),UpdatedSkeletonNode::Binary), (NodeIndex::from(11), UpdatedSkeletonNode::Binary)]
)]
#[case::deleted_left_child(
    &NodeIndex::from(5),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[(20, 0)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge { path_to_bottom: PathToBottom::RIGHT_CHILD }),
    &[(NodeIndex::from(11),UpdatedSkeletonNode::Binary)]
)]
#[case::deleted_two_children(
    &NodeIndex::from(5),
    &TempSkeletonNode::Empty,
    &TempSkeletonNode::Empty,
    &[(20, 0), (22, 0)],
    TempSkeletonNode::Empty,
    &[]
)]
#[case::left_edge_right_deleted(
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Edge { path_to_bottom: PathToBottom::RIGHT_CHILD }),
    &TempSkeletonNode::Empty,
    &[(22, 0)],
    TempSkeletonNode::Original(OriginalSkeletonNode::Edge { path_to_bottom: PathToBottom::from("01") }),
    &[]
)]
fn test_node_from_binary_data(
    #[case] root_index: &NodeIndex,
    #[case] left: &TempSkeletonNode,
    #[case] right: &TempSkeletonNode,
    #[case] _leaf_modifications: &[(u128, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(TreeHeight::MAX, _leaf_modifications)] mut updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut expected_skeleton_tree = updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = updated_skeleton.node_from_binary_data(root_index, left, right);
    assert_eq!(temp_node, expected_node);
    assert_eq!(updated_skeleton.skeleton_tree, expected_skeleton_tree);
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
        OriginalSkeletonNode::Edge {path_to_bottom: PathToBottom::from("11")}
    ),
    &[],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge { path_to_bottom: (PathToBottom::from("0011")) }
    ),
    &[],
)]
#[case::to_edge_sibling(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(5),
    &TempSkeletonNode::Original(OriginalSkeletonNode::EdgeSibling(
        EdgeData {bottom_hash: HashOutput::ZERO, path_to_bottom: PathToBottom::from("01")}
    )),
   &[],
    TempSkeletonNode::Original(OriginalSkeletonNode::EdgeSibling(
        EdgeData {bottom_hash: HashOutput::ZERO, path_to_bottom: (PathToBottom::from("101"))}
    )),
    &[],
)]
#[case::to_binary(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Binary),
    &[],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge {path_to_bottom: PathToBottom::RIGHT_CHILD}
    ),
    &[(NodeIndex::from(7), UpdatedSkeletonNode::Binary)]
)]
#[case::to_non_empty_leaf(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &[(7, 1)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge {path_to_bottom: PathToBottom::RIGHT_CHILD}
    ),
    &[]
)]
fn test_node_from_edge_data(
    #[case] path: &PathToBottom,
    #[case] bottom_index: &NodeIndex,
    #[case] bottom: &TempSkeletonNode,
    #[case] _leaf_modifications: &[(u128, u8)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(TreeHeight::MAX, _leaf_modifications)] mut updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let mut expected_skeleton_tree = updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let temp_node = updated_skeleton.node_from_edge_data(path, bottom_index, bottom);
    assert_eq!(temp_node, expected_node);
    assert_eq!(updated_skeleton.skeleton_tree, expected_skeleton_tree);
}
