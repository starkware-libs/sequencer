use crate::hash::types::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::skeleton_node::SkeletonNode;
use crate::types::Felt;

pub(crate) trait TreeHashFunction<L: LeafDataTrait, H: HashFunction> {
    /// Computes the hash of given input.
    async fn compute_node_hash(skeleton_node: SkeletonNode<L>) -> HashOutput;
}

// TODO(Amos, 01/05/2024): Implement types for NodeIndex, EdgePath, EdgePathLength
#[allow(dead_code)]
pub(crate) struct NodeIndex(pub Felt);

#[allow(dead_code)]
pub(crate) struct EdgePath(pub Felt);

#[allow(dead_code)]
pub(crate) struct EdgePathLength(pub u16);

#[allow(dead_code)]
pub(crate) struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
}

pub(crate) trait LeafDataTrait {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}
