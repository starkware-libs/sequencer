use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, PathToBottom};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// A node in the structure of a Patricia-Merkle tree, before the update.
pub(crate) enum OriginalSkeletonNode {
    Binary,
    Edge(PathToBottom),
    // Represents a root of a subtree where none of it's descendants has changed.
    UnmodifiedSubTree(HashOutput),
}

/// A representation of the data required to build an original skeleton node.
// TODO(Nimrod, 20/6/2024): Delete this enum.
pub(crate) enum OriginalSkeletonInputNode {
    Binary { hash: HashOutput, data: BinaryData },
    Edge(EdgeData),
    Leaf(HashOutput),
}
