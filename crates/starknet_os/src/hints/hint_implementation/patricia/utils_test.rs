use std::collections::HashMap;

use assert_matches::assert_matches;
use num_bigint::BigUint;
use starknet_api::hash::HashOutput;
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
    get_descents,
    patricia_guess_descents,
    CanonicNode,
    DecodeNodeCase,
    DescentMap,
    DescentStart,
    InnerNode,
    LayerIndex,
    Path,
    PatriciaError,
    Preimage,
    PreimageMap,
    UpdateTree,
};

/// Builds a full preimage map for a binary tree of the given height.
/// The root hash (must be greater than 0) is the first node, and the left and right children are
/// calculated as left = root * 2 and right = left + 1.
/// For example, for height 2, and root 1, the tree looks like this:
/// ```text
///          1
///    2          3
///  4    5     6   7
fn build_full_preimage_map(height: SubTreeHeight, root: HashOutput) -> PreimageMap {
    assert!(root != HashOutput(Felt::ZERO));

    let mut preimage_map = PreimageMap::new();
    let left = HashOutput(root.0 * Felt::TWO);
    let right = HashOutput(left.0 + Felt::ONE);

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
/// The root hash must be greater than 0.
/// The edge node is from the right child of the root, to it most left descendant.
/// For example, for height 3, and root 1, the tree looks like this:
/// ```text
///          1
///    2          3
///  4    5     x   x
/// 8 9 10 11 12 x x x
fn build_preimage_map_with_edge_node(height: SubTreeHeight, root: HashOutput) -> PreimageMap {
    assert!(root != HashOutput(Felt::ZERO));
    assert!(height.0 >= 2, "Height must be at least 2 to create an edge node.");

    let mut preimage_map = PreimageMap::new();
    let left = HashOutput(root.0 * Felt::TWO);
    let right = HashOutput(left.0 + Felt::ONE);

    let next_height = SubTreeHeight(height.0 - 1);
    let bottom_hash = HashOutput(Felt::from(u128::try_from(right.0).unwrap() << next_height.0));

    preimage_map.insert(root, Preimage::Binary(BinaryData { left_hash: left, right_hash: right }));

    preimage_map.extend(build_full_preimage_map(next_height, left));
    preimage_map.insert(
        right,
        Preimage::Edge(EdgeData {
            bottom_hash,
            path_to_bottom: PathToBottom::new(
                EdgePath::new_u128(0),
                EdgePathLength::new(next_height.0).unwrap(),
            )
            .unwrap(),
        }),
    );

    preimage_map
}

/// Builds a full update_tree of the given height.
/// All the leaves are set to 42.
fn build_full_tree(height: SubTreeHeight, path: Path) -> UpdateTree {
    if height.0 == 0 {
        return UpdateTree::Leaf(HashOutput(Felt::from(42)));
    }

    UpdateTree::InnerNode(InnerNode::Both(
        Box::new(build_full_tree(SubTreeHeight(height.0 - 1), path.turn_left().unwrap())),
        Box::new(build_full_tree(SubTreeHeight(height.0 - 1), path.turn_right().unwrap())),
    ))
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
    let case = inner_node.case();
    assert_matches!(left_child, UpdateTree::Leaf(value) if value.0 == leaf_left.0);
    assert_eq!(right_child, &UpdateTree::None);
    assert_matches!(case, DecodeNodeCase::Left);

    // Right node.
    let inner_node = InnerNode::Right(Box::new(UpdateTree::Leaf(leaf_right)));
    let (left_child, right_child) = inner_node.get_children();
    let case = inner_node.case();
    assert_eq!(left_child, &UpdateTree::None);
    assert_matches!(right_child, UpdateTree::Leaf(value) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Right);

    // Two children.
    let inner_node = InnerNode::Both(
        Box::new(UpdateTree::Leaf(leaf_left)),
        Box::new(UpdateTree::Leaf(leaf_right)),
    );
    let (left_child, right_child) = inner_node.get_children();
    let case = inner_node.case();
    assert_matches!(left_child, UpdateTree::Leaf(value) if value.0 == leaf_left.0);
    assert_matches!(right_child, UpdateTree::Leaf(value) if value.0 == leaf_right.0);
    assert_matches!(case, DecodeNodeCase::Both);
}

#[test]
fn test_new_canonic_node() {
    //          1
    //    2          3
    //  4    5     x   x
    // 8 9 10 11 12 x x x
    let preimage_map = build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::ONE));

    // Binary.
    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::ONE));
    assert_eq!(node_1, CanonicNode::BinaryOrLeaf(HashOutput(Felt::ONE)));

    // Edge.
    let node_3 = CanonicNode::new(&preimage_map, &HashOutput(Felt::THREE));
    let edge_data_3 = EdgeData {
        bottom_hash: HashOutput(Felt::from(12)),
        path_to_bottom: PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap())
            .unwrap(),
    };
    assert_eq!(node_3, CanonicNode::Edge(edge_data_3));

    // Leaf / not in preimage_map.
    let node_8 = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(8)));
    assert_eq!(node_8, CanonicNode::BinaryOrLeaf(HashOutput(Felt::from(8))));

    // Empty.
    let node_empty = CanonicNode::new(&preimage_map, &HashOutput(Felt::ZERO));
    assert_eq!(node_empty, CanonicNode::Empty);
}

#[test]
fn test_get_children() {
    //          1
    //    2          3
    //  4    5     x   x
    // 8 9 10 11 12 x x x
    let preimage_map = build_preimage_map_with_edge_node(SubTreeHeight(3), HashOutput(Felt::ONE));

    let node_1 = CanonicNode::new(&preimage_map, &HashOutput(Felt::ONE));
    let node_2 = CanonicNode::new(&preimage_map, &HashOutput(Felt::TWO));
    let node_3 = CanonicNode::new(&preimage_map, &HashOutput(Felt::THREE));
    let node_6 = CanonicNode::Edge(EdgeData {
        bottom_hash: HashOutput(Felt::from(12)),
        path_to_bottom: PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(1).unwrap())
            .unwrap(),
    });

    let children_1 = get_children(&node_1, &preimage_map).unwrap();
    let children_3 = get_children(&node_3, &preimage_map).unwrap();
    assert_eq!(children_1, (node_2, node_3));
    assert_eq!(children_3, (node_6, CanonicNode::Empty));

    // Empty node.
    let node_empty = CanonicNode::new(&preimage_map, &HashOutput(Felt::ZERO));
    let children_empty = get_children(&node_empty, &preimage_map).unwrap();
    assert_eq!(children_empty, (CanonicNode::Empty, CanonicNode::Empty));

    // Not in preimage_map.
    let node = CanonicNode::new(&preimage_map, &HashOutput(Felt::from(100)));
    let result = get_children(&node, &preimage_map);
    assert_matches!(result, Err(PatriciaError::MissingPreimage(_)));
}

#[test]
fn test_node_path_turns() {
    // path = 01
    let path =
        Path(PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(2).unwrap()).unwrap());

    // Turn left -> path = 010
    let path = path.turn_left().unwrap();
    assert_eq!(
        path,
        Path(PathToBottom::new(EdgePath::new_u128(2), EdgePathLength::new(3).unwrap()).unwrap())
    );

    // Turn right -> path = 0101
    let path = path.turn_right().unwrap();
    assert_eq!(
        path,
        Path(PathToBottom::new(EdgePath::new_u128(5), EdgePathLength::new(4).unwrap()).unwrap())
    );

    let path =
        Path(PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(251).unwrap()).unwrap());
    let result = path.turn_left();
    assert_matches!(result, Err(PatriciaError::EdgePath(_)));
    let result = path.turn_right();
    assert_matches!(result, Err(PatriciaError::EdgePath(_)));
}

#[test]
fn test_node_path_remove_first_edges() {
    // path = 0101
    let path =
        Path(PathToBottom::new(EdgePath::new_u128(5), EdgePathLength::new(4).unwrap()).unwrap());

    let new_path = path.remove_first_edges(EdgePathLength::new(0).unwrap()).unwrap();
    assert_eq!(new_path, path);

    let new_path = path.remove_first_edges(EdgePathLength::new(1).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath::new_u128(5), EdgePathLength::new(3).unwrap()).unwrap())
    );

    let new_path = path.remove_first_edges(EdgePathLength::new(2).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(2).unwrap()).unwrap())
    );

    let new_path = path.remove_first_edges(EdgePathLength::new(3).unwrap()).unwrap();
    assert_eq!(
        new_path,
        Path(PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(1).unwrap()).unwrap())
    );

    let result = path.remove_first_edges(EdgePathLength::new(5).unwrap());
    assert_matches!(result, Err(PatriciaError::PathToBottom(_)));
}

#[test]
fn test_get_descents_empty() {
    let mut descent_map = DescentMap::new();

    get_descents(
        &mut descent_map,
        DescentStart {
            height: SubTreeHeight(1),
            path_to_upper_node: Path(PathToBottom::new_zero()),
        },
        &HashMap::new(),
        (&UpdateTree::None, CanonicNode::Empty, CanonicNode::Empty),
    )
    .unwrap();
    assert!(descent_map.is_empty());
}

/// The descent map for a full tree should be empty.
#[test]
fn test_guess_descents_full_tree() {
    // Create a full tree of height 3
    let height = SubTreeHeight(3);
    let update_tree = build_full_tree(height, Path(PathToBottom::new_zero()));

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));
    let mut preimage_map = build_full_preimage_map(height, HashOutput(Felt::ONE));
    preimage_map.extend(build_full_preimage_map(height, HashOutput(Felt::from(16))));

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert!(descent_map.is_empty());
}

#[test]
fn test_guess_descents_update_one_leaf() {
    // previous tree:
    //        1
    //    x       x
    //  x   x   x   x
    // x 9 x x x x x x
    // new tree:
    //         16
    //     x        x
    //  x     x   x   x
    // x 129 x x x x x x

    let height = SubTreeHeight(3);
    let update_tree = build_update_tree(
        height,
        vec![(LayerIndex(BigUint::from(1u128)), (HashOutput(Felt::from(129))))],
    )
    .unwrap();

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));

    let prev_leaf = HashOutput(Felt::from(9));
    let new_leaf = HashOutput(Felt::from(129));

    let path_to_bottom =
        PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(3).unwrap()).unwrap();

    let mut preimage_map = PreimageMap::new();
    preimage_map
        .insert(prev_root, Preimage::Edge(EdgeData { bottom_hash: prev_leaf, path_to_bottom }));
    preimage_map
        .insert(new_root, Preimage::Edge(EdgeData { bottom_hash: new_leaf, path_to_bottom }));

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert_eq!(
        descent_map,
        DescentMap::from([(
            DescentStart {
                height: SubTreeHeight(3),
                path_to_upper_node: Path(PathToBottom::new_zero())
            },
            Path(
                PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(3).unwrap()).unwrap(),
            ),
        )]),
    );
}

#[test]
fn test_guess_descents_update_two_adjacent_leaves() {
    // previous tree:
    //        1
    //    x       x
    //  4   x   x   x
    // 8 9 x x x x x x
    // new tree:
    //           16
    //       x        x
    //    64    x   x   x
    // 128 129 x x x x x x

    let height = SubTreeHeight(3);
    let update_tree = build_update_tree(
        height,
        vec![
            (LayerIndex(BigUint::from(0u128)), (HashOutput(Felt::from(128)))),
            (LayerIndex(BigUint::from(1u128)), (HashOutput(Felt::from(129)))),
        ],
    )
    .unwrap();

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));

    let prev_inner_node = HashOutput(Felt::from(4));
    let new_inner_node = HashOutput(Felt::from(64));

    let path_to_bottom =
        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap();

    let mut preimage_map = PreimageMap::new();
    preimage_map.insert(
        prev_root,
        Preimage::Edge(EdgeData { bottom_hash: prev_inner_node, path_to_bottom }),
    );
    preimage_map
        .insert(new_root, Preimage::Edge(EdgeData { bottom_hash: new_inner_node, path_to_bottom }));
    preimage_map.insert(
        prev_inner_node,
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(8)),
            right_hash: HashOutput(Felt::from(9)),
        }),
    );
    preimage_map.insert(
        new_inner_node,
        Preimage::Binary(BinaryData {
            left_hash: HashOutput(Felt::from(128)),
            right_hash: HashOutput(Felt::from(129)),
        }),
    );

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert_eq!(
        descent_map,
        DescentMap::from([(
            DescentStart {
                height: SubTreeHeight(3),
                path_to_upper_node: Path(PathToBottom::new_zero())
            },
            Path(
                PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap(),
            ),
        )]),
    );
}

#[test]
fn test_guess_descents_update_two_leaves() {
    // previous tree:
    //        1
    //    2        3
    //  x   x    x   x
    // x 9 x x 12 x x x
    // new tree:
    //          16
    //      32        33
    //  x     x     x    x
    // x 129 x x 132  x x x

    let height = SubTreeHeight(3);
    let update_tree = build_update_tree(
        height,
        vec![
            (LayerIndex(BigUint::from(1u128)), (HashOutput(Felt::from(129)))),
            (LayerIndex(BigUint::from(4u128)), (HashOutput(Felt::from(132)))),
        ],
    )
    .unwrap();

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));

    let prev_left_inner_node = HashOutput(Felt::from(2));
    let new_left_inner_node = HashOutput(Felt::from(32));

    let prev_right_inner_node = HashOutput(Felt::from(3));
    let new_right_inner_node = HashOutput(Felt::from(33));

    let prev_left_leaf = HashOutput(Felt::from(9));
    let new_left_leaf = HashOutput(Felt::from(129));

    let prev_right_leaf = HashOutput(Felt::from(12));
    let new_right_leaf = HashOutput(Felt::from(132));

    let left_path_to_bottom =
        PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(2).unwrap()).unwrap();
    let right_path_to_bottom =
        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap();

    let mut preimage_map = PreimageMap::new();
    preimage_map.insert(
        prev_root,
        Preimage::Binary(BinaryData {
            left_hash: prev_left_inner_node,
            right_hash: prev_right_inner_node,
        }),
    );
    preimage_map.insert(
        new_root,
        Preimage::Binary(BinaryData {
            left_hash: new_left_inner_node,
            right_hash: new_right_inner_node,
        }),
    );
    preimage_map.insert(
        prev_left_inner_node,
        Preimage::Edge(EdgeData {
            bottom_hash: prev_left_leaf,
            path_to_bottom: left_path_to_bottom,
        }),
    );
    preimage_map.insert(
        new_left_inner_node,
        Preimage::Edge(EdgeData {
            bottom_hash: new_left_leaf,
            path_to_bottom: left_path_to_bottom,
        }),
    );
    preimage_map.insert(
        prev_right_inner_node,
        Preimage::Edge(EdgeData {
            bottom_hash: prev_right_leaf,
            path_to_bottom: right_path_to_bottom,
        }),
    );
    preimage_map.insert(
        new_right_inner_node,
        Preimage::Edge(EdgeData {
            bottom_hash: new_right_leaf,
            path_to_bottom: right_path_to_bottom,
        }),
    );

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert_eq!(
        descent_map,
        DescentMap::from([
            (
                DescentStart {
                    height: SubTreeHeight(2),
                    path_to_upper_node: Path(
                        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(1).unwrap())
                            .unwrap(),
                    ),
                },
                Path(left_path_to_bottom),
            ),
            (
                DescentStart {
                    height: SubTreeHeight(2),
                    path_to_upper_node: Path(
                        PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(1).unwrap())
                            .unwrap(),
                    ),
                },
                Path(right_path_to_bottom),
            )
        ]),
    );
}

/// Tests the case where prev tree and new tree both have an edge node, but it's not the same
/// edge node. In this case, the descent path is the common subpath of the two edge nodes.
/// Note that it's the common subpath of the edge nodes of the three trees - update_tree has an
/// edge node from the root to the parent of the swapped leaves.
#[test]
fn test_guess_descents_change_leaf() {
    // previous tree:
    //        1
    //    x       x
    //  x   x   x   x
    // 8 x x x x x x x
    // new tree:
    //         16
    //     x         x
    //  x    x     x   x
    // x 129 x x  x x x x

    let height = SubTreeHeight(3);
    let update_tree = build_update_tree(
        height,
        vec![
            (LayerIndex(BigUint::from(0u128)), (HashOutput(Felt::from(0)))),
            (LayerIndex(BigUint::from(1u128)), (HashOutput(Felt::from(129)))),
        ],
    )
    .unwrap();

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));

    let prev_leaf = HashOutput(Felt::from(8));
    let new_leaf = HashOutput(Felt::from(129));

    let prev_path =
        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(3).unwrap()).unwrap();
    let new_path =
        PathToBottom::new(EdgePath::new_u128(1), EdgePathLength::new(3).unwrap()).unwrap();

    let mut preimage_map = PreimageMap::new();
    preimage_map.insert(
        prev_root,
        Preimage::Edge(EdgeData { bottom_hash: prev_leaf, path_to_bottom: prev_path }),
    );
    preimage_map.insert(
        new_root,
        Preimage::Edge(EdgeData { bottom_hash: new_leaf, path_to_bottom: new_path }),
    );

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert_eq!(
        descent_map,
        DescentMap::from([(
            DescentStart {
                height: SubTreeHeight(3),
                path_to_upper_node: Path(PathToBottom::new_zero()),
            },
            Path(
                PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap(),
            ),
        ),]),
    );
}

/// It also tests the case of a new edge node in the new tree (switching the trees), since the
/// preimage map is the same and there's no difference in handling previous or new tree.
/// Note that the values of the modifications aren't important.
#[test]
fn test_guess_descents_split_edge_node() {
    // previous tree:
    //        1
    //    x       x
    //  x   x   x   x
    // 8 x x x x x x x
    // new tree:
    //            16
    //       x         x
    //   64    x     x   x
    // 8  129 x x   x x x x

    let height = SubTreeHeight(3);
    let update_tree = build_update_tree(
        height,
        vec![(LayerIndex(BigUint::from(1u128)), (HashOutput(Felt::from(129))))],
    )
    .unwrap();

    let prev_root = HashOutput(Felt::ONE);
    let new_root = HashOutput(Felt::from(16));

    let new_inner_node = HashOutput(Felt::from(64));

    let left_leaf = HashOutput(Felt::from(8));
    let right_leaf = HashOutput(Felt::from(129));

    let prev_path =
        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(3).unwrap()).unwrap();
    let new_path =
        PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap();

    let mut preimage_map = PreimageMap::new();
    preimage_map.insert(
        prev_root,
        Preimage::Edge(EdgeData { bottom_hash: left_leaf, path_to_bottom: prev_path }),
    );
    preimage_map.insert(
        new_root,
        Preimage::Edge(EdgeData { bottom_hash: new_inner_node, path_to_bottom: new_path }),
    );
    preimage_map.insert(
        new_inner_node,
        Preimage::Binary(BinaryData { left_hash: left_leaf, right_hash: right_leaf }),
    );

    let descent_map =
        patricia_guess_descents(height, &update_tree, &preimage_map, prev_root, new_root).unwrap();
    assert_eq!(
        descent_map,
        DescentMap::from([(
            DescentStart {
                height: SubTreeHeight(3),
                path_to_upper_node: Path(PathToBottom::new_zero()),
            },
            Path(
                PathToBottom::new(EdgePath::new_u128(0), EdgePathLength::new(2).unwrap()).unwrap(),
            ),
        ),]),
    );
}
