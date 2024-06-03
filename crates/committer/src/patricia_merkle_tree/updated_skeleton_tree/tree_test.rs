use std::collections::HashMap;

use rstest::rstest;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::{LeafModifications, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree, UpdatedSkeletonTreeImpl,
};

#[rstest]
// TODO(Tzahi, 15/6/2024): Add tests.
fn test_updated_skeleton_tree_impl_create() {
    let mut original_skeleton = OriginalSkeletonTreeImpl {
        nodes: HashMap::from([(
            NodeIndex::ROOT,
            OriginalSkeletonNode::Edge(PathToBottom::from("0".repeat(251).as_str())),
        )]),
    };
    let leaf_modifications = LeafModifications::from([(NodeIndex::FIRST_LEAF, SkeletonLeaf::Zero)]);
    let updated_skeleton_tree =
        UpdatedSkeletonTreeImpl::create(&mut original_skeleton, &leaf_modifications).unwrap();
    assert!(updated_skeleton_tree.skeleton_tree.is_empty());
}
