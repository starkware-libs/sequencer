use assert_matches::assert_matches;
use ethnum::U256;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use super::{
    build_update_tree,
    get_children,
    preimage_tree,
    CanonicChildren,
    CanonicNode,
    DecodeNodeCase,
    InnerNode,
    LayerIndex,
    Path,
    PatriciaError,
    Preimage,
    PreimageMap,
    PreimageNode,
    UpdateTree,
};

fn build_full_preimage_map(height: SubTreeHeight, root: HashOutput) -> PreimageMap {
    let mut preimage_map = PreimageMap::new();
    let left = HashOutput(root.0 * Felt::from(2));
    let right = HashOutput(left.0 + Felt::from(1));

    preimage_map.insert(root, Preimage::Binary(BinaryData { left_hash: left, right_hash: right }));

    // We can stop at height 1, the leaf nodes are not relevant.
    if height.0 > 1 {
        let next_height = SubTreeHeight(height.0 - 1);
        preimage_map.extend(build_full_preimage_map(next_height, left));
        preimage_map.extend(build_full_preimage_map(next_height, right));
    }

    preimage_map
}

/// Builds a preimage map with an edge node at the specified height.
/// The edge node is from the right child of the root, to it most left descendant.
fn build_preimage_map_with_edge_node(height: SubTreeHeight, root: HashOutput) -> PreimageMap {
    if height.0 < 2 {
        panic!("Height must be at least 2 to create an edge node.");
    }

    let mut preimage_map = PreimageMap::new();
    let left = HashOutput(root.0 * Felt::from(2));
    let right = HashOutput(left.0 + Felt::from(1));

    let next_height = SubTreeHeight(height.0 - 1);
    let bottom_hash = HashOutput(Felt::from(u128::try_from(right.0).unwrap() << next_height.0));

    preimage_map.insert(root, Preimage::Binary(BinaryData { left_hash: left, right_hash: right }));

    preimage_map.extend(build_full_preimage_map(next_height, left));
    preimage_map.insert(
        right,
        Preimage::Edge(EdgeData {
            bottom_hash,
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::ZERO),
                EdgePathLength::new(next_height.0).unwrap(),
            )
            .unwrap(),
        }),
    );

    preimage_map
}

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

#[test]
fn test_new_canonic_node() {
    //          1
    //    2          3
    //  4    5     6   x
    // 8 9 10 11 12 x x x
    let preimage_map =
        build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::from(1)));
    // Binary.
    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(1)));
    assert_eq!(node_1, CanonicNode::BinaryOrLeaf(HashOutput(Felt::from(1))));

    // Edge.
    let node_3 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(3)));
    let edge_data_3 = EdgeData {
        bottom_hash: HashOutput(Felt::from(12)),
        path_to_bottom: PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(2).unwrap())
            .unwrap(),
    };
    assert_eq!(node_3, CanonicNode::Edge(edge_data_3));

    // Leaf / not in preimage_map.
    let node_8 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(8)));
    assert_eq!(node_8, CanonicNode::BinaryOrLeaf(HashOutput(Felt::from(8))));
}

#[test]
fn test_get_children() {
    //          1
    //    2          3
    //  4    5     6   x
    // 8 9 10 11 12 x x x
    let preimage_map =
        build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::from(1)));

    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(1)));
    let node_2 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(2)));
    let node_3 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(3)));
    let node_6 = CanonicNode::Edge(EdgeData {
        bottom_hash: HashOutput(Felt::from(12)),
        path_to_bottom: PathToBottom::new(EdgePath(U256::ZERO), EdgePathLength::new(1).unwrap())
            .unwrap(),
    });

    let children_1 = get_children(&node_1, &preimage_map).unwrap();
    let children_3 = get_children(&node_3, &preimage_map).unwrap();
    assert_eq!(children_1, CanonicChildren { left: Some(node_2), right: Some(node_3) });
    assert_eq!(children_3, CanonicChildren { left: Some(node_6), right: None });

    // Empty node.
    let node_empty = CanonicNode::BinaryOrLeaf(HashOutput(Felt::from(0)));
    let children_empty = get_children(&node_empty, &preimage_map).unwrap();
    assert_eq!(
        children_empty,
        CanonicChildren { left: Some(node_empty.clone()), right: Some(node_empty) }
    );

    // Not in preimage_map.
    let node = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(100)));
    let result = get_children(&node, &preimage_map);
    assert_matches!(result, Err(PatriciaError::MissingPreimage(_)));
}

#[test]
fn test_preimage_tree() {
    //          1
    //    2          3
    //  4    5     6   x
    // 8 9 10 11 12 x x x
    let preimage_map =
        build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::from(1)));
    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(1)));

    let mut iter_1 = preimage_tree(SubTreeHeight(3), &preimage_map, node_1);
    let mut iter_3;
    let mut iter_6;
    let mut iter_12;

    let branch_1 = iter_1.next().unwrap().unwrap();
    match branch_1 {
        PreimageNode::Branch { left, right } => {
            assert_matches!(left, Some(_));
            iter_3 = right.unwrap();
        }
        _ => panic!("Expected branch"),
    }

    let branch_3 = iter_3.next().unwrap().unwrap();
    match branch_3 {
        PreimageNode::Branch { left, right } => {
            iter_6 = left.unwrap();
            assert!(right.is_none());
        }
        _ => panic!("Expected branch"),
    }

    let branch_6 = iter_6.next().unwrap().unwrap();
    match branch_6 {
        PreimageNode::Branch { left, right } => {
            iter_12 = left.unwrap();
            assert!(right.is_none());
        }
        _ => panic!("Expected branch"),
    }

    let leaf_12 = iter_12.next().unwrap().unwrap();
    assert_matches!(leaf_12, PreimageNode::Leaf);

    assert_matches!(iter_12.next(), None);
}

#[test]
fn test_preimage_tree_get_children() {
    //          1
    //    2          3
    //  4    5     6   x
    // 8 9 10 11 12 x x x
    let preimage_map =
        build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::from(1)));
    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(1)));

    // Branch.
    let mut iter_1 = preimage_tree(SubTreeHeight(3), &preimage_map, node_1);
    let mut iter_3;

    let children_1 = iter_1.get_children().unwrap();
    match children_1 {
        (Some(_), Some(right)) => {
            iter_3 = right;
        }
        _ => panic!("Expected children"),
    }

    let children_3 = iter_3.get_children().unwrap();
    assert_matches!(children_3, (Some(_), None));

    // Leaf.
    let node_12 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(12)));
    let mut iter_12 = preimage_tree(SubTreeHeight(0), &preimage_map, node_12);
    match iter_12.get_children() {
        Err(PatriciaError::ExpectedBranch(s)) if s == "Leaf" => (),
        _ => panic!("Expected ExpectedBranch error with 'Leaf' message"),
    }
}

#[test]
fn test_node_path_turns() {
    // path = 01
    let path =
        Path(PathToBottom::new(EdgePath(U256::new(1)), EdgePathLength::new(2).unwrap()).unwrap());

    // Turn left -> path = 010
    let path = path.turn_left().unwrap();
    assert_eq!(
        path,
        Path(PathToBottom::new(EdgePath(U256::new(2)), EdgePathLength::new(3).unwrap()).unwrap())
    );

    // Turn right -> path = 0101
    let path = path.turn_right().unwrap();
    assert_eq!(
        path,
        Path(PathToBottom::new(EdgePath(U256::new(5)), EdgePathLength::new(4).unwrap()).unwrap())
    );

    let path =
        Path(PathToBottom::new(EdgePath(U256::new(0)), EdgePathLength::new(251).unwrap()).unwrap());
    let result = path.turn_left();
    assert_matches!(result, Err(PatriciaError::EdgePath(_)));
    let result = path.turn_right();
    assert_matches!(result, Err(PatriciaError::EdgePath(_)));
}

#[test]
fn test_node_path_remove_first_edges() {
    // path = 0101
    let path =
        Path(PathToBottom::new(EdgePath(U256::new(5)), EdgePathLength::new(4).unwrap()).unwrap());

    let new_path = path.remove_first_edges(EdgePathLength::new(0).unwrap()).unwrap();
    assert_eq!(new_path, path);

    let new_path = path.remove_first_edges(EdgePathLength::new(1).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath(U256::new(5)), EdgePathLength::new(3).unwrap()).unwrap())
    );

    let new_path = path.remove_first_edges(EdgePathLength::new(2).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath(U256::new(1)), EdgePathLength::new(2).unwrap()).unwrap())
    );

    let new_path = path.remove_first_edges(EdgePathLength::new(3).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath(U256::new(1)), EdgePathLength::new(1).unwrap()).unwrap())
    );

    let result = path.remove_first_edges(EdgePathLength::new(5).unwrap());
    assert_matches!(result, Err(PatriciaError::PathToBottom(_)));
}
