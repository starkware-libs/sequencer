use assert_matches::assert_matches;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use super::{build_update_tree, LayerIndex, UpdateTree, UpdateTreeInner};

#[test]
fn test_build_update_tree_empty() {
    let update_tree = build_update_tree(SubTreeHeight::new(3), vec![]).unwrap();
    assert_matches!(update_tree, None);
}

fn print_update_tree(index: u64, update_tree: &UpdateTree) {
    match update_tree {
        None => {}
        Some(UpdateTreeInner::InnerNode(left, right)) => {
            print_update_tree(index * 2, left);
            print_update_tree(index * 2 + 1, right);
        }
        Some(UpdateTreeInner::Leaf(value)) => println!("{index}: {}", value.0),
    }
}

#[test]
fn test_build_update_tree() {
    let modifications = vec![
        (LayerIndex::from(1u128), HashOutput(Felt::from(12))),
        (LayerIndex::from(4u128), HashOutput(Felt::from(1000))),
        (LayerIndex::from(6u128), HashOutput(Felt::from(30))),
    ];
    let update_tree = build_update_tree(SubTreeHeight::new(3), modifications).unwrap();
    print_update_tree(0, &update_tree);

    // expected_update_tree = (((None, 12), None), ((1000, None), (30, None)))
    let expected_update_tree = Some(UpdateTreeInner::InnerNode(
        Box::new(Some(UpdateTreeInner::InnerNode(
            Box::new(Some(UpdateTreeInner::InnerNode(
                Box::new(None),
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(12))))),
            ))),
            Box::new(None),
        ))),
        Box::new(Some(UpdateTreeInner::InnerNode(
            Box::new(Some(UpdateTreeInner::InnerNode(
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(1000))))),
                Box::new(None),
            ))),
            Box::new(Some(UpdateTreeInner::InnerNode(
                Box::new(Some(UpdateTreeInner::Leaf(HashOutput(Felt::from(30))))),
                Box::new(None),
            ))),
        ))),
    ));

    assert_eq!(update_tree, expected_update_tree);
}
