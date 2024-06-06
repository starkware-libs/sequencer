use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

/// A node in the structure of a Patricia-Merkle tree, after the update.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UpdatedSkeletonNode {
    Binary,
    Edge(PathToBottom),
    // All unmodified nodes on the merkle paths of modified leaves.
    Sibling(HashOutput),
    // Unmodified bottom of edge nodes on the merkle paths of modified leaves.
    UnmodifiedBottom(HashOutput),
    Leaf,
}
