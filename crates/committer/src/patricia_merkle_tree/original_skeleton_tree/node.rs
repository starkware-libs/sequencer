use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// A node in the structure of a Patricia-Merkle tree, before the update.
pub(crate) enum OriginalSkeletonNode {
    Binary,
    Edge { path_to_bottom: PathToBottom },
    // Unmodified leaf / binary nodes on the merkle paths of modified leaves.
    LeafOrBinarySibling(HashOutput),
    // Unmodified edge siblings bottom nodes on the merkle paths of modified leaves.
    UnmodifiedBottom(HashOutput),
    Leaf(SkeletonLeaf),
}
