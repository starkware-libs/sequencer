use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

/// A node in the structure of a Patricia-Merkle tree, after the update.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatedSkeletonNode {
    Binary,
    Edge(PathToBottom),
    // Represents a root of a subtree where none of it's descendants has changed.
    UnmodifiedSubTree(HashOutput),
    Leaf,
}
