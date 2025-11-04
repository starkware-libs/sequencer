use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// A node in the structure of a Patricia-Merkle tree, before the update.
pub enum OriginalSkeletonNode {
    Binary,
    Edge(PathToBottom),
    // Represents a root of a subtree where none of it's descendants has changed.
    UnmodifiedSubTree(HashOutput),
}
