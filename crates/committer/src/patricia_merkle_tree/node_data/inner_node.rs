use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::errors::{EdgePathError, PathToBottomError};
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};

use ethnum::U256;
use strum_macros::{EnumDiscriminants, EnumIter};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testing"), derive(EnumDiscriminants))]
#[cfg_attr(any(test, feature = "testing"), strum_discriminants(derive(EnumIter)))]
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
pub struct EdgePath(pub U256);

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
#[derive(
    Clone, Copy, Debug, Default, PartialOrd, derive_more::Add, derive_more::Sub, PartialEq, Eq, Hash,
)]
pub struct EdgePathLength(u8);

impl EdgePathLength {
    /// [EdgePathLength] constant that represents the longest path (from some node) in a tree.
    pub const ONE: Self = Self(1);
    pub const MAX: Self = Self(TreeHeight::MAX.0);

    pub fn new(length: u8) -> Result<Self, EdgePathError> {
        if length > Self::MAX.0 {
            return Err(EdgePathError::IllegalLength { length });
        }
        Ok(Self(length))
    }
}

impl From<EdgePathLength> for u8 {
    fn from(value: EdgePathLength) -> Self {
        value.0
    }
}

impl From<EdgePathLength> for Felt {
    fn from(value: EdgePathLength) -> Self {
        value.0.into()
    }
}

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
    pub(crate) const LEFT_CHILD: Self = Self {
        path: EdgePath(U256::ZERO),
        length: EdgePathLength(1),
    };

    pub(crate) const RIGHT_CHILD: Self = Self {
        path: EdgePath(U256::ONE),
        length: EdgePathLength(1),
    };

    pub(crate) fn bottom_index(&self, root_index: NodeIndex) -> NodeIndex {
        NodeIndex::compute_bottom_index(root_index, self)
    }

    /// Returns true iff the first step on the path is to the left.
    pub(crate) fn is_left_descendant(&self) -> bool {
        self.path.0 >> (self.length.0 - 1) == 0
    }

    pub(crate) fn concat_paths(&self, other: Self) -> Self {
        Self {
            path: EdgePath::from((self.path.0 << other.length.0) + other.path.0),
            length: self.length + other.length,
        }
    }

    /// Returns the path after removing the first steps (the edges from the path's origin node).
    pub(crate) fn remove_first_edges(
        &self,
        n_edges: EdgePathLength,
    ) -> Result<Self, PathToBottomError> {
        if self.length <= n_edges {
            return Err(PathToBottomError::RemoveEdgesError {
                length: self.length,
                n_edges,
            });
        }
        Ok(Self {
            path: EdgePath(self.path.0 >> n_edges.0),
            length: self.length - n_edges,
        })
    }
}
