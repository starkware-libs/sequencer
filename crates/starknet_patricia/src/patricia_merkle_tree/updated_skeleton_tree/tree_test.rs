use std::collections::HashMap;

use rstest::{fixture, rstest};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::internal_test_utils::{
    get_initial_updated_skeleton,
    MockLeaf,
    OriginalSkeletonMockTrieConfig,
};
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTree,
    OriginalSkeletonTreeImpl,
};
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};

#[allow(clippy::as_conversions)]
const TREE_HEIGHT: usize = SubTreeHeight::ACTUAL_HEIGHT.0 as usize;

#[fixture]
fn initial_updated_skeleton(
    #[default(&[])] original_skeleton: &[(NodeIndex, OriginalSkeletonNode)],
    #[default(&[])] leaf_modifications: &[(NodeIndex, u8)],
) -> UpdatedSkeletonTreeImpl {
    get_initial_updated_skeleton(original_skeleton, leaf_modifications)
}

#[rstest]
#[case::empty_to_empty_illegal_modifications(&[], &[(NodeIndex::FIRST_LEAF, 0)], &[])]
#[case::empty_to_edge(
    &[],
    &[(NodeIndex::FIRST_LEAF, 1)],
    &[
        (NodeIndex::ROOT,
        UpdatedSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str())))
    ],
)]
#[case::empty_to_binary(
    &[],
    &[(NodeIndex::FIRST_LEAF, 1), (NodeIndex::FIRST_LEAF + 1, 1)],
    &([
        (NodeIndex::FIRST_LEAF >> 1, UpdatedSkeletonNode::Binary),
        (NodeIndex::ROOT,
        UpdatedSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT - 1).as_str()))),
    ]),
)]
#[case::nonempty_to_empty_tree(
    &[
        (NodeIndex::ROOT,
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str())))
    ],
    &[(NodeIndex::FIRST_LEAF, 0)],
    &[]
)]
#[case::non_empty_to_binary(
    &[
        (NodeIndex::ROOT,
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str())),
    )],
    &[
        (NodeIndex::FIRST_LEAF, 1),
        (NodeIndex::FIRST_LEAF + 1, 1)
    ],
    &[
        (
            NodeIndex::ROOT,
            UpdatedSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT - 1).as_str()))
        ),
        (NodeIndex::FIRST_LEAF >> 1, UpdatedSkeletonNode::Binary)
    ]
)]
#[case::non_empty_replace_edge_bottom(
    &[
        (NodeIndex::ROOT,
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str())),
    )],
    &[
        (NodeIndex::FIRST_LEAF, 0),
        (NodeIndex::FIRST_LEAF + 1, 1)
    ],
    &[
        (NodeIndex::ROOT,
        UpdatedSkeletonNode::Edge(PathToBottom::from(("0".repeat(TREE_HEIGHT - 1) + "1").as_str())))
    ]
)]
#[case::fake_modification(
    &[
        (NodeIndex::ROOT,
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str())),
    )],
    &[
        (NodeIndex::FIRST_LEAF, 1),
    ],
    &[
        (NodeIndex::ROOT,
        UpdatedSkeletonNode::Edge(PathToBottom::from(("0".repeat(TREE_HEIGHT)).as_str())))
    ]
)]
#[case::fake_deletion(
    &[
        (NodeIndex::ROOT,
        OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(TREE_HEIGHT).as_str()))),
        (NodeIndex::FIRST_LEAF,
            OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::from(1_u8))))
    ],
    &[
        (NodeIndex::FIRST_LEAF + 1, 0),
    ],
    &[
        (NodeIndex::ROOT,
        UpdatedSkeletonNode::Edge(PathToBottom::from(("0".repeat(TREE_HEIGHT)).as_str())))
    ]
)]
fn test_updated_skeleton_tree_impl_create(
    #[case] original_skeleton: &[(NodeIndex, OriginalSkeletonNode)],
    #[case] leaf_modifications: &[(NodeIndex, u8)],
    #[case] expected_skeleton_additions: &[(NodeIndex, UpdatedSkeletonNode)],
    #[with(original_skeleton, leaf_modifications)]
    initial_updated_skeleton: UpdatedSkeletonTreeImpl,
) {
    let leaf_modifications: LeafModifications<SkeletonLeaf> =
        leaf_modifications.iter().map(|(index, val)| (*index, (*val).into())).collect();
    let mut leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut leaf_indices);
    let mut original_skeleton = OriginalSkeletonTreeImpl {
        nodes: original_skeleton.iter().cloned().collect(),
        sorted_leaf_indices,
    };
    let updated_skeleton_tree =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton, &leaf_modifications).unwrap();

    let mut expected_skeleton_tree = initial_updated_skeleton.skeleton_tree.clone();
    expected_skeleton_tree.extend(expected_skeleton_additions.iter().cloned());

    assert_eq!(updated_skeleton_tree.skeleton_tree, expected_skeleton_tree);
}

#[rstest]
#[case::empty_modifications(HashMap::new())]
#[case::non_empty_modifications(HashMap::from([(NodeIndex::FIRST_LEAF + NodeIndex::from(7), MockLeaf::default())]))]
fn test_updated_empty_tree(#[case] modifications: LeafModifications<MockLeaf>) {
    let mut storage = HashMap::new();
    let map_storage = MapStorage { storage: &mut storage };
    let mut indices: Vec<NodeIndex> = modifications.keys().copied().collect();
    let mut original_skeleton = OriginalSkeletonTreeImpl::create(
        &map_storage,
        HashOutput::ROOT_OF_EMPTY_TREE,
        SortedLeafIndices::new(&mut indices),
        &OriginalSkeletonMockTrieConfig::new(false),
        &modifications,
    )
    .unwrap();

    let skeleton_modifications =
        modifications.into_iter().map(|(idx, leaf)| (idx, leaf.0.into())).collect();
    let updated_skeleton_tree =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton, &skeleton_modifications).unwrap();
    assert!(updated_skeleton_tree.is_empty());
}
