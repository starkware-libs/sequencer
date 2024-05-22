use ethnum::U256;
use rstest::{fixture, rstest};
use std::collections::HashMap;

use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{
    EdgeData, EdgePath, EdgePathLength, PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::compute_updated_skeleton_tree::TempSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::patricia_merkle_tree::{
    original_skeleton_tree::tree::OriginalSkeletonTreeImpl, types::TreeHeight,
};

fn empty_skeleton(height: u8) -> OriginalSkeletonTreeImpl {
    OriginalSkeletonTreeImpl {
        nodes: HashMap::new(),
        tree_height: TreeHeight::new(height),
    }
}

#[fixture]
fn updated_skeleton() -> UpdatedSkeletonTreeImpl {
    UpdatedSkeletonTreeImpl {
        skeleton_tree: HashMap::new(),
    }
}

#[rstest]
#[case::small_tree_positive(
    3, 2, vec![NodeIndex::from(8),NodeIndex::from(10),NodeIndex::from(11)], true)]
#[case::small_tree_negative(3, 2, vec![NodeIndex::from(10),NodeIndex::from(11)], false)]
#[case::large_tree_farthest_leaves(
    251,
    1,
    vec![NodeIndex::ROOT << 251, NodeIndex::MAX_INDEX],
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
    #[case] tree_height: u8,
    #[case] root_index: u8,
    #[case] leaf_indices: Vec<NodeIndex>,
    #[case] expected: bool,
) {
    let skeleton_tree = empty_skeleton(tree_height);
    let root_index = NodeIndex::new(root_index.into());
    assert_eq!(
        skeleton_tree.has_leaves_on_both_sides(&root_index, &leaf_indices),
        expected
    );
}

#[rstest]
#[case::first_leaf_not_descendant(3, 3, vec![NodeIndex::from(8), NodeIndex::from(12)])]
#[case::last_leaf_not_descendant(3, 2, vec![NodeIndex::from(8), NodeIndex::from(12)])]
#[should_panic(expected = "is not a descendant of the root")]
fn test_has_leaves_on_both_sides_assertions(
    #[case] tree_height: u8,
    #[case] root_index: u8,
    #[case] leaf_indices: Vec<NodeIndex>,
) {
    let skeleton_tree = empty_skeleton(tree_height);
    let root_index = NodeIndex::new(root_index.into());
    skeleton_tree.has_leaves_on_both_sides(&root_index, &leaf_indices);
}

#[rstest]
#[case::small_tree_single_leaf(3, 1, vec![U256::from(8_u8)], PathToBottom {path:EdgePath(Felt::ZERO), length:EdgePathLength(3)})]
#[case::small_tree_few_leaves(
    3, 1, vec![U256::from(12_u8),U256::from(13_u8),U256::from(14_u8)], PathToBottom {path:EdgePath(Felt::ONE), length:EdgePathLength(1)})]
#[case::small_tree_few_leaves2(
    3, 1, vec![U256::from(12_u8),U256::from(13_u8)], PathToBottom {path:EdgePath(Felt::from(2_u8)), length:EdgePathLength(2)})]
#[case::large_tree_positive_consecutive_indices_of_different_sides(
    251,
    1,
    vec![(U256::from(3u8) << 250) - U256::ONE, U256::from(3u8) << 250],
    PathToBottom {path:EdgePath(Felt::ZERO), length:EdgePathLength(0)})]
#[case::large_tree_positive_consecutive_indices(
    251,
    3<<126,
    vec![U256::from(3u8) << 250, (U256::from(3u8) << 250)+ U256::ONE],
    PathToBottom {path:EdgePath(Felt::ZERO), length:EdgePathLength(123)})]
fn test_get_path_to_lca(
    #[case] tree_height: u8,
    #[case] root_index: u128,
    #[case] leaf_indices: Vec<U256>,
    #[case] expected: PathToBottom,
) {
    let skeleton_tree = empty_skeleton(tree_height);
    let root_index = NodeIndex::new(root_index.into());
    assert_eq!(
        skeleton_tree.get_path_to_lca(
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
#[case::to_empty_leaf(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &[(NodeIndex::from(7),SkeletonLeaf::Zero)],
    TempSkeletonNode::Empty,
    &[],
)]
#[case::to_non_empty_leaf(
    &PathToBottom::RIGHT_CHILD,
    &NodeIndex::from(7),
    &TempSkeletonNode::Original(OriginalSkeletonNode::Leaf(SkeletonLeaf::NonZero)),
    &[(NodeIndex::from(7), SkeletonLeaf::NonZero)],
    TempSkeletonNode::Original(
        OriginalSkeletonNode::Edge {path_to_bottom: PathToBottom::RIGHT_CHILD}
    ),
    &[(NodeIndex::from(7), UpdatedSkeletonNode::Leaf(SkeletonLeaf::NonZero))]
)]
fn test_node_from_edge_data(
    mut updated_skeleton: UpdatedSkeletonTreeImpl,
    #[case] path: &PathToBottom,
    #[case] bottom_index: &NodeIndex,
    #[case] bottom: &TempSkeletonNode,
    #[case] leaf_modifications: &[(NodeIndex, SkeletonLeaf)],
    #[case] expected_node: TempSkeletonNode,
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
) {
    let mut expected_skeleton_tree = updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());
    let leaf_modifications: HashMap<NodeIndex, SkeletonLeaf> =
        leaf_modifications.iter().cloned().collect();
    let temp_node =
        updated_skeleton.node_from_edge_data(path, bottom_index, bottom, &leaf_modifications);
    assert_eq!(temp_node, expected_node);
    assert_eq!(updated_skeleton.skeleton_tree, expected_skeleton_tree);
}
