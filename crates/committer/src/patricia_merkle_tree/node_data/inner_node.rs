use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::NodeIndex;

#[derive(Clone, Debug, PartialEq, Eq)]
// A Patricia-Merkle tree node's data, i.e., the pre-image of its hash.
pub enum NodeData<L: LeafData> {
    Binary(BinaryData),
    Edge(EdgeData),
    Leaf(L),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BinaryData {
    pub left_hash: HashOutput,
    pub right_hash: HashOutput,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgePath(pub Felt);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgePathLength(pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgeData {
    pub bottom_hash: HashOutput,
    pub path_to_bottom: PathToBottom,
}

impl PathToBottom {
    pub(crate) fn bottom_index(&self, root_index: NodeIndex) -> NodeIndex {
        NodeIndex::compute_bottom_index(root_index, self)
    }
}
