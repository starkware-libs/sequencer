use assert_matches::assert_matches;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use super::{build_update_tree, Children, InnerNode, LayerIndex, UpdateTreeInner};
use crate::hints::hint_implementation::patricia::error::PatriciaError;
use crate::hints::hint_implementation::patricia::utils::DecodeNodeCase;

#[test]
fn test_build_update_tree_empty() {
    let update_tree = build_update_tree(SubTreeHeight::new(3), vec![]).unwrap();
    assert!(update_tree.is_none());
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
    let expected_update_tree = Some(UpdateTreeInner::InnerNode(InnerNode::Both(
        Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
            UpdateTreeInner::InnerNode(InnerNode::Right(Box::new(Some(UpdateTreeInner::Leaf(
                HashOutput(Felt::from(12)),
            ))))),
        )))))),
        Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Both(
            Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
                UpdateTreeInner::Leaf(HashOutput(Felt::from(1000))),
            )))))),
            Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
                UpdateTreeInner::Leaf(HashOutput(Felt::from(30))),
            )))))),
        )))),
    )));

    assert_eq!(update_tree, expected_update_tree);
}

#[test]
fn test_decode_node() {
    let leaf_left = HashOutput(Felt::from(252));
    let leaf_right = HashOutput(Felt::from(3000));

    // Left node
    let inner_node = InnerNode::new(Some(UpdateTreeInner::Leaf(leaf_left)), None).unwrap();
    let (Children { left_child, right_child }, case) = inner_node.decode_node();
    assert_matches!(left_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_left.0);
    assert!(right_child.is_none());
    assert_matches!(case, DecodeNodeCase::Left);

    // Right node
    let inner_node = InnerNode::new(None, Some(UpdateTreeInner::Leaf(leaf_right))).unwrap();
    let (Children { left_child, right_child }, case) = inner_node.decode_node();
    assert!(left_child.is_none());
    assert_matches!(right_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Right);

    // Two children
    let inner_node = InnerNode::new(
        Some(UpdateTreeInner::Leaf(leaf_left)),
        Some(UpdateTreeInner::Leaf(leaf_right)),
    )
    .unwrap();
    let (Children { left_child, right_child }, case) = inner_node.decode_node();
    assert_matches!(left_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_left.0);
    assert_matches!(right_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Both);

    // No children
    let inner_node = InnerNode::new(None, None);
    assert_matches!(inner_node, Err(PatriciaError::InvalidInnerNode));
}

#[test]
fn test_decode_node_tree() {
    let modifications = vec![
        (LayerIndex::from(1u128), HashOutput(Felt::from(12))),
        (LayerIndex::from(4u128), HashOutput(Felt::from(1000))),
        (LayerIndex::from(6u128), HashOutput(Felt::from(30))),
    ];

    let update_tree = build_update_tree(SubTreeHeight::new(3), modifications).unwrap();
    let inner_node: InnerNode = match update_tree.unwrap() {
        UpdateTreeInner::InnerNode(inner_node) => inner_node,
        _ => panic!("Expected InnerNode"),
    };
    let (Children { left_child, right_child }, case) = inner_node.decode_node();

    let expected_left_child = Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
        UpdateTreeInner::InnerNode(InnerNode::Right(Box::new(Some(UpdateTreeInner::Leaf(
            HashOutput(Felt::from(12)),
        ))))),
    )))));

    let expected_right_child = Some(UpdateTreeInner::InnerNode(InnerNode::Both(
        Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
            UpdateTreeInner::Leaf(HashOutput(Felt::from(1000))),
        )))))),
        Box::new(Some(UpdateTreeInner::InnerNode(InnerNode::Left(Box::new(Some(
            UpdateTreeInner::Leaf(HashOutput(Felt::from(30))),
        )))))),
    )));

    assert_eq!(left_child, &expected_left_child);
    assert_eq!(right_child, &expected_right_child);
    assert_matches!(case, DecodeNodeCase::Both);
}
