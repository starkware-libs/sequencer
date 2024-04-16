use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::types::{EdgeData, LeafDataTrait, PathToBottom};

#[allow(dead_code)]
/// A node in the structure of a Patricia-Merkle tree, before the update.
pub(crate) enum OriginalSkeletonNode<L: LeafDataTrait> {
    Binary,
    Edge { path_to_bottom: PathToBottom },
    // Unmodified leaf / binary nodes on the merkle paths of modified leaves.
    LeafOrBinarySibling(HashOutput),
    // Unmodified edge nodes on the merkle paths of modified leaves.
    EdgeSibling(EdgeData),
    Leaf(L),
    Empty,
}
