use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use ethnum::U256;

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

// Wraps a U256. Maximal possible value is the longest path in a tree of height 251 (2 ^ 251 - 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgePath(U256);

impl EdgePath {
    pub const BITS: u8 = TreeHeight::MAX.0;

    /// [EdgePath] constant that represents the longest path (from some node) in a tree.
    #[allow(clippy::as_conversions)]
    pub const MAX: Self = Self(U256::from_words(
        u128::MAX >> (U256::BITS - Self::BITS as u32),
        u128::MAX,
    ));
}

impl From<U256> for EdgePath {
    fn from(value: U256) -> Self {
        assert!(value <= EdgePath::MAX.0, "Path {value:?} is too long.");
        Self(value)
    }
}

impl From<u128> for EdgePath {
    fn from(value: u128) -> Self {
        Self(value.into())
    }
}

impl From<&EdgePath> for Felt {
    fn from(path: &EdgePath) -> Self {
        Self::from_bytes_be(&path.0.to_be_bytes())
    }
}

impl From<&EdgePath> for U256 {
    fn from(path: &EdgePath) -> Self {
        path.0
    }
}
#[derive(Clone, Copy, Debug, Default, derive_more::Add, PartialEq, Eq, Hash)]
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
    #[allow(dead_code)]
    pub(crate) const LEFT_CHILD: Self = Self {
        path: EdgePath(U256::ZERO),
        length: EdgePathLength(1),
    };

    #[allow(dead_code)]
    pub(crate) const RIGHT_CHILD: Self = Self {
        path: EdgePath(U256::ONE),
        length: EdgePathLength(1),
    };

    pub(crate) fn bottom_index(&self, root_index: NodeIndex) -> NodeIndex {
        NodeIndex::compute_bottom_index(root_index, self)
    }

    pub(crate) fn concat_paths(&self, other: Self) -> Self {
        Self {
            path: EdgePath::from((self.path.0 << other.length.0) + other.path.0),
            length: self.length + other.length,
        }
    }
}
