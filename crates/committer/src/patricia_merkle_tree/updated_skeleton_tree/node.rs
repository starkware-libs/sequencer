use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;

#[allow(dead_code)]
/// A node in the structure of a Patricia-Merkle tree, after the update.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UpdatedSkeletonNode {
    Binary,
    Edge { path_to_bottom: PathToBottom },
    // All unmodified nodes on the merkle paths of modified leaves.
    Sibling(HashOutput),
    Leaf(SkeletonLeaf),
}
