use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use super::{build_update_tree, InnerNode, LayerIndex, UpdateTreeInner};

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
