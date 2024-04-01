use crate::hash::types::HashOutput;
use crate::patricia_merkle_tree::types::{LeafTrait, PathToBottom};

#[allow(dead_code)]
pub(crate) enum SkeletonNode<L: LeafTrait> {
    Binary,
    Edge { path_to_bottom: PathToBottom },
    Sibling(HashOutput),
    Leaf(L),
    Empty,
}
