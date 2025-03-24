use assert_matches::assert_matches;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_types_core::felt::Felt;

use super::{build_update_tree, Height, TreeIndex, UpdateTree, UpdateTreeInner};

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
