use assert_matches::assert_matches;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use super::{build_update_tree, DecodeNodeCase, InnerNode, LayerIndex, UpdateTree};

#[test]
fn test_build_update_tree_empty() {
    let update_tree = build_update_tree(SubTreeHeight::new(3), vec![]).unwrap();
    assert_eq!(update_tree, UpdateTree::None);
}

#[test]
fn test_build_update_tree() {
    let modifications = vec![
        (LayerIndex::from(1u128), HashOutput(Felt::from(12))),
        (LayerIndex::from(4u128), HashOutput(Felt::from(1000))),
        (LayerIndex::from(6u128), HashOutput(Felt::from(30))),
    ];
    let update_tree = build_update_tree(SubTreeHeight::new(3), modifications).unwrap();

    // expected_update_tree = (((None, 12), None), ((1000, None), (30, None)))
    let expected_update_tree = UpdateTree::InnerNode(InnerNode::Both(
        Box::new(UpdateTree::InnerNode(InnerNode::Left(Box::new(UpdateTree::InnerNode(
            InnerNode::Right(Box::new(UpdateTree::Leaf(HashOutput(Felt::from(12))))),
        ))))),
        Box::new(UpdateTree::InnerNode(InnerNode::Both(
            Box::new(UpdateTree::InnerNode(InnerNode::Left(Box::new(UpdateTree::Leaf(
                HashOutput(Felt::from(1000)),
            ))))),
            Box::new(UpdateTree::InnerNode(InnerNode::Left(Box::new(UpdateTree::Leaf(
                HashOutput(Felt::from(30)),
            ))))),
        ))),
    ));

    assert_eq!(update_tree, expected_update_tree);
}

#[test]
fn test_inner_node() {
    let leaf_left = HashOutput(Felt::from(252));
    let leaf_right = HashOutput(Felt::from(3000));

    // Left node.
    let inner_node = InnerNode::Left(Box::new(UpdateTree::Leaf(leaf_left)));
    let (left_child, right_child) = inner_node.get_children();
    let case = DecodeNodeCase::from(&inner_node);
    assert_matches!(left_child, UpdateTree::Leaf(value) if value.0 == leaf_left.0);
    assert_eq!(right_child, &UpdateTree::None);
    assert_matches!(case, DecodeNodeCase::Left);

    // Right node.
    let inner_node = InnerNode::Right(Box::new(UpdateTree::Leaf(leaf_right)));
    let (left_child, right_child) = inner_node.get_children();
    let case = DecodeNodeCase::from(&inner_node);
    assert_eq!(left_child, &UpdateTree::None);
    assert_matches!(right_child, UpdateTree::Leaf(value) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Right);

    // Two children.
    let inner_node = InnerNode::Both(
        Box::new(UpdateTree::Leaf(leaf_left)),
        Box::new(UpdateTree::Leaf(leaf_right)),
    );
    let (left_child, right_child) = inner_node.get_children();
    let case = DecodeNodeCase::from(&inner_node);
    assert_matches!(left_child, UpdateTree::Leaf(value) if value.0 == leaf_left.0);
    assert_matches!(right_child, UpdateTree::Leaf(value) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Both);
}
