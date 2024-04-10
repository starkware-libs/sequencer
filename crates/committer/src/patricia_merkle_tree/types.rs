use crate::hash::types::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::filled_node::NodeData;
use crate::types::Felt;

#[cfg(test)]
#[path = "test_utils.rs"]
mod test_utils;
#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

pub(crate) trait TreeHashFunction<L: LeafDataTrait, H: HashFunction> {
    /// Computes the hash of given node data.
    fn compute_node_hash(node_data: NodeData<L>) -> HashOutput;
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeIndex(pub Felt);

#[allow(dead_code)]
impl NodeIndex {
    pub(crate) fn root_index() -> NodeIndex {
        NodeIndex(Felt::ONE)
    }

    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        NodeIndex(index.0 * Felt::TWO.pow(length.0) + path.0)
    }
}

#[allow(dead_code)]
pub(crate) struct EdgePath(pub Felt);

#[allow(dead_code)]
pub(crate) struct EdgePathLength(pub u8);

#[allow(dead_code)]
pub(crate) struct TreeHeight(pub u8);

#[allow(dead_code)]
pub(crate) struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
}

#[allow(dead_code)]
pub(crate) struct EdgeData {
    bottom_hash: HashOutput,
    path_to_bottom: PathToBottom,
}

pub(crate) trait LeafDataTrait {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;
}
