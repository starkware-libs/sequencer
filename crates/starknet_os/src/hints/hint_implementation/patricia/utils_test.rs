use assert_matches::assert_matches;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_types_core::felt::Felt;

use super::{
    build_update_tree,
    decode_node,
    DecodeNodeCase,
    DecodedNode,
    Height,
    TreeIndex,
    UpdateTree,
    UpdateTreeInner,
};
use crate::hints::hint_implementation::patricia::error::PatriciaError;

#[test]
fn test_build_update_tree_empty() {
    let update_tree = build_update_tree(Height(3), vec![]);
    assert_matches!(update_tree, None);
}

fn print_update_tree(index: u64, update_tree: &UpdateTree) {
    match update_tree {
        None => {}
        Some(UpdateTreeInner::Tuple(left, right)) => {
            print_update_tree(index * 2, left);
            print_update_tree(index * 2 + 1, right);
        }
        Some(UpdateTreeInner::Leaf(value)) => println!("{index}: {}", value.0),
    }
}

#[test]
fn test_build_update_tree() {
    let modifications = vec![
        (TreeIndex::from(1u64), HashOutput(Felt::from(12))),
        (TreeIndex::from(4u64), HashOutput(Felt::from(1000))),
        (TreeIndex::from(6u64), HashOutput(Felt::from(30))),
    ];
    let update_tree = build_update_tree(Height(3), modifications);
    print_update_tree(0, &update_tree);

    // expected_update_tree = (((None, 12), None), ((1000, None), (30, None)))
    let expected_update_tree = Some(UpdateTreeInner::Tuple(
        Box::new(Some(UpdateTreeInner::Tuple(
            Box::new(Some(UpdateTreeInner::Tuple(
                Box::new(None),
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(12))))),
            ))),
            Box::new(None),
        ))),
        Box::new(Some(UpdateTreeInner::Tuple(
            Box::new(Some(UpdateTreeInner::Tuple(
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(1000))))),
                Box::new(None),
            ))),
            Box::new(Some(UpdateTreeInner::Tuple(
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(30))))),
                Box::new(None),
            ))),
        ))),
    ));

    assert_eq!(update_tree, expected_update_tree);
}

#[test]
fn test_decode_node() {
    let leaf_left = HashOutput(Felt::from(252));
    let leaf_right = HashOutput(Felt::from(3000));

    // Left node
    let node =
        UpdateTreeInner::Tuple(Box::new(Some(UpdateTreeInner::Leaf(leaf_left))), Box::new(None));
    let DecodedNode { left_child, right_child, case } = decode_node(&node).unwrap();
    assert_matches!(left_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_left.0);
    assert_matches!(right_child, None);
    assert_matches!(case, DecodeNodeCase::Left);

    // Right node
    let node =
        UpdateTreeInner::Tuple(Box::new(None), Box::new(Some(UpdateTreeInner::Leaf(leaf_right))));
    let DecodedNode { left_child, right_child, case } = decode_node(&node).unwrap();
    assert_matches!(left_child, None);
    assert_matches!(right_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Right);

    // Two children
    let node = UpdateTreeInner::Tuple(
        Box::new(Some(UpdateTreeInner::Leaf(leaf_left))),
        Box::new(Some(UpdateTreeInner::Leaf(leaf_right))),
    );
    let DecodedNode { left_child, right_child, case } = decode_node(&node).unwrap();
    assert_matches!(left_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_left.0);
    assert_matches!(right_child, Some(UpdateTreeInner::Leaf(value)) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Both);

    // No children
    let node = UpdateTreeInner::Tuple(Box::new(None), Box::new(None));
    let result = decode_node(&node);
    assert_matches!(result, Err(PatriciaError::IsEmpty));

    // Leaf
    let node = UpdateTreeInner::Leaf(leaf_left);
    let result = decode_node(&node);
    assert_matches!(result, Err(PatriciaError::IsLeaf));
}
