use std::collections::HashMap;

use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::hash::pedersen::PedersenHashFunction;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, FilledNode};
use crate::patricia_merkle_tree::filled_tree::tree::FilledTree;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData, EdgeData, EdgePath, EdgePathLength, NodeData, PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::LeafDataImpl;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree, UpdatedSkeletonTreeImpl,
};

#[tokio::test(flavor = "multi_thread")]
/// This test is a sanity test for computing the root hash of the patricia merkle tree with a single node that is a leaf with hash==1.
async fn test_filled_tree_sanity() {
    let mut skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<LeafDataImpl>> = HashMap::new();
    skeleton_tree.insert(
        NodeIndex::ROOT,
        UpdatedSkeletonNode::Leaf(LeafDataImpl::CompiledClassHash(ClassHash(Felt::ONE))),
    );
    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };
    let root_hash = updated_skeleton_tree
        .compute_filled_tree::<PedersenHashFunction, TreeHashFunctionImpl<PedersenHashFunction>>()
        .await
        .unwrap()
        .get_root_hash()
        .unwrap();
    assert_eq!(root_hash, HashOutput(Felt::ONE), "Root hash mismatch");
}

// TODO(Aner, 11/4/25): Add test with large patricia merkle tree.
// TOOD(Aner, 11/4/25): Add test with different leaf types.

#[tokio::test(flavor = "multi_thread")]
/// This test is a small test for testing the root hash computation of the patricia merkle tree.
/// The tree structure & results were computed seperately and tested for regression.
///                                i=1: binary
///                                /        \
///                        i=2: edge      i=3: edge
///                        l=1, p=0       l=4, p=15
///                      /                      \
///                 i=4: binary                  \
///                /           \                  \
///            i=8: edge    i=9: edge              \
///            l=2, p=3     l=2, p=0                \
///               \              /                   \
///                \            /                     \
///            i=35: leaf   i=36: leaf               i=63: leaf
///                  v=1          v=2                      v=3
async fn test_small_filled_tree() {
    // Set up the updated skeleton tree.
    let nodes_in_skeleton_tree = [
        create_binary_updated_skeleton_node_for_testing(1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(2, 0, 1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(3, 15, 4),
        create_binary_updated_skeleton_node_for_testing(4),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(8, 3, 2),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(9, 0, 2),
        create_leaf_updated_skeleton_node_for_testing(35, "0x1"),
        create_leaf_updated_skeleton_node_for_testing(36, "0x2"),
        create_leaf_updated_skeleton_node_for_testing(63, "0x3"),
    ];
    let skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<LeafDataImpl>> =
        nodes_in_skeleton_tree.into_iter().collect();

    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };

    // Compute the hash values.
    let filled_tree = updated_skeleton_tree
        .compute_filled_tree::<PedersenHashFunction, TreeHashFunctionImpl<PedersenHashFunction>>()
        .await
        .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash().unwrap();

    // The expected hash values were computed separately.
    let expected_root_hash = HashOutput(
        Felt::from_hex("0xe8899e8c731a35f5e9ce4c4bc32aabadcc81c5cdcc1aeba74fa7509046c338").unwrap(),
    );
    let expected_filled_tree_map = HashMap::from([
        create_binary_entry_for_testing(
            1,
            "0xe8899e8c731a35f5e9ce4c4bc32aabadcc81c5cdcc1aeba74fa7509046c338",
            "0x4e970ad06a06486b44fff5606c4f65486d31e05e323d65a618d4ef8cdf6d3a0",
            "0x2955a96b09495fb2ce4ed65cf679c54e54aefc2c6972d7f3042590000bb7543",
        ),
        create_edge_entry_for_testing(
            2,
            "0x4e970ad06a06486b44fff5606c4f65486d31e05e323d65a618d4ef8cdf6d3a0",
            0,
            1,
            "0x5d36a1ae900ef417a5696417dde9a0244b873522f40b552e4a60acde0991bc9",
        ),
        create_edge_entry_for_testing(
            3,
            "0x2955a96b09495fb2ce4ed65cf679c54e54aefc2c6972d7f3042590000bb7543",
            15,
            4,
            "0x3",
        ),
        create_binary_entry_for_testing(
            4,
            "0x5d36a1ae900ef417a5696417dde9a0244b873522f40b552e4a60acde0991bc9",
            "0x582d984e4005c27b9c886cd00ec9a82ed5323aa629f6ea6b3ed7c0386ae6256",
            "0x39eb7b85bcc9deac314406d6b73154b09b008f8af05e2f58ab623f4201d0b88",
        ),
        create_edge_entry_for_testing(
            8,
            "0x582d984e4005c27b9c886cd00ec9a82ed5323aa629f6ea6b3ed7c0386ae6256",
            3,
            2,
            "0x1",
        ),
        create_edge_entry_for_testing(
            9,
            "0x39eb7b85bcc9deac314406d6b73154b09b008f8af05e2f58ab623f4201d0b88",
            0,
            2,
            "0x2",
        ),
        create_leaf_entry_for_testing(35, "0x1"),
        create_leaf_entry_for_testing(36, "0x2"),
        create_leaf_entry_for_testing(63, "0x3"),
    ]);
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
}

#[tokio::test(flavor = "multi_thread")]
/// This test is a small test for testing the root hash computation of the patricia merkle tree
/// with sibling nodes. The tree structure & results are a partial of test_small_filled_tree.
///                   i=1: binary
///                   /        \
///            i=2: edge      i=3: sibling
///            l=1, p=0       hash=0x2955a96b09495fb2ce4ed65cf679c54e54aefc2c6972d7f3042590000bb7543
///                /                      
///            i=4: binary                  
///          /           \                  
///      i=8: edge    i=9: sibling           
///      l=2, p=3     hash=0x39eb7b85bcc9deac314406d6b73154b09b008f8af05e2f58ab623f4201d0b88
///           \             
///            \            
///         i=35: leaf   
///            v=1    
async fn test_small_tree_with_sibling_nodes() {
    // Set up the updated skeleton tree.
    let nodes_in_skeleton_tree = [
        create_binary_updated_skeleton_node_for_testing(1),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(2, 0, 1),
        create_sibling_updated_skeleton_node_for_testing(
            3,
            "0x2955a96b09495fb2ce4ed65cf679c54e54aefc2c6972d7f3042590000bb7543",
        ),
        create_binary_updated_skeleton_node_for_testing(4),
        create_path_to_bottom_edge_updated_skeleton_node_for_testing(8, 3, 2),
        create_sibling_updated_skeleton_node_for_testing(
            9,
            "0x39eb7b85bcc9deac314406d6b73154b09b008f8af05e2f58ab623f4201d0b88",
        ),
        create_leaf_updated_skeleton_node_for_testing(35, "0x1"),
    ];
    let skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<LeafDataImpl>> =
        nodes_in_skeleton_tree.into_iter().collect();

    let updated_skeleton_tree = UpdatedSkeletonTreeImpl { skeleton_tree };

    // Compute the hash values.
    let filled_tree = updated_skeleton_tree
        .compute_filled_tree::<PedersenHashFunction, TreeHashFunctionImpl<PedersenHashFunction>>()
        .await
        .unwrap();
    let filled_tree_map = filled_tree.get_all_nodes();
    let root_hash = filled_tree.get_root_hash().unwrap();

    // The expected hash values were computed separately. Note that the sibling nodes are not
    // computed in the filled tree, but the hash values are directly used. The hashes of sibling
    // nodes should not appear in the filled tree.
    let expected_root_hash = HashOutput(
        Felt::from_hex("0xe8899e8c731a35f5e9ce4c4bc32aabadcc81c5cdcc1aeba74fa7509046c338").unwrap(),
    );
    let expected_filled_tree_map = HashMap::from([
        create_binary_entry_for_testing(
            1,
            "0xe8899e8c731a35f5e9ce4c4bc32aabadcc81c5cdcc1aeba74fa7509046c338",
            "0x4e970ad06a06486b44fff5606c4f65486d31e05e323d65a618d4ef8cdf6d3a0",
            "0x2955a96b09495fb2ce4ed65cf679c54e54aefc2c6972d7f3042590000bb7543",
        ),
        create_edge_entry_for_testing(
            2,
            "0x4e970ad06a06486b44fff5606c4f65486d31e05e323d65a618d4ef8cdf6d3a0",
            0,
            1,
            "0x5d36a1ae900ef417a5696417dde9a0244b873522f40b552e4a60acde0991bc9",
        ),
        create_binary_entry_for_testing(
            4,
            "0x5d36a1ae900ef417a5696417dde9a0244b873522f40b552e4a60acde0991bc9",
            "0x582d984e4005c27b9c886cd00ec9a82ed5323aa629f6ea6b3ed7c0386ae6256",
            "0x39eb7b85bcc9deac314406d6b73154b09b008f8af05e2f58ab623f4201d0b88",
        ),
        create_edge_entry_for_testing(
            8,
            "0x582d984e4005c27b9c886cd00ec9a82ed5323aa629f6ea6b3ed7c0386ae6256",
            3,
            2,
            "0x1",
        ),
        create_leaf_entry_for_testing(35, "0x1"),
    ]);
    assert_eq!(filled_tree_map, &expected_filled_tree_map);
    assert_eq!(root_hash, expected_root_hash, "Root hash mismatch");
}

fn create_binary_updated_skeleton_node_for_testing(
    index: u128,
) -> (NodeIndex, UpdatedSkeletonNode<LeafDataImpl>) {
    (NodeIndex::from(index), UpdatedSkeletonNode::Binary)
}

fn create_path_to_bottom_edge_updated_skeleton_node_for_testing(
    index: u128,
    path: u128,
    length: u8,
) -> (NodeIndex, UpdatedSkeletonNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        UpdatedSkeletonNode::Edge {
            path_to_bottom: PathToBottom {
                path: EdgePath(Felt::from(path)),
                length: EdgePathLength(length),
            },
        },
    )
}

fn create_sibling_updated_skeleton_node_for_testing(
    index: u128,
    hash: &str,
) -> (NodeIndex, UpdatedSkeletonNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        UpdatedSkeletonNode::Sibling(HashOutput(Felt::from_hex(hash).unwrap())),
    )
}

fn create_leaf_updated_skeleton_node_for_testing(
    index: u128,
    value: &str,
) -> (NodeIndex, UpdatedSkeletonNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        UpdatedSkeletonNode::Leaf(LeafDataImpl::CompiledClassHash(ClassHash(
            Felt::from_hex(value).unwrap(),
        ))),
    )
}

fn create_binary_entry_for_testing(
    index: u128,
    hash: &str,
    left_hash: &str,
    right_hash: &str,
) -> (NodeIndex, FilledNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Binary(BinaryData {
                left_hash: HashOutput(Felt::from_hex(left_hash).unwrap()),
                right_hash: HashOutput(Felt::from_hex(right_hash).unwrap()),
            }),
        },
    )
}

fn create_edge_entry_for_testing(
    index: u128,
    hash: &str,
    path: u128,
    length: u8,
    bottom_hash: &str,
) -> (NodeIndex, FilledNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Edge(EdgeData {
                bottom_hash: HashOutput(Felt::from_hex(bottom_hash).unwrap()),
                path_to_bottom: PathToBottom {
                    path: EdgePath(Felt::from(path)),
                    length: EdgePathLength(length),
                },
            }),
        },
    )
}

fn create_leaf_entry_for_testing(index: u128, hash: &str) -> (NodeIndex, FilledNode<LeafDataImpl>) {
    (
        NodeIndex::from(index),
        FilledNode {
            hash: HashOutput(Felt::from_hex(hash).unwrap()),
            data: NodeData::Leaf(LeafDataImpl::CompiledClassHash(ClassHash(
                Felt::from_hex(hash).unwrap(),
            ))),
        },
    )
}
