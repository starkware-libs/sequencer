use std::collections::HashMap;

use ethnum::U256;
use starknet_api::hash::HashOutput;
use starknet_rust_core::types::MerkleNode;
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::node_data::errors::{
    EdgePathError,
    PathToBottomError,
    PreimageError,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::types::{NodeIndex, SubTreeHeight};

#[cfg(test)]
#[path = "inner_node_tests.rs"]
pub mod inner_node_test;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testing"), derive(strum_macros::EnumDiscriminants))]
#[cfg_attr(any(test, feature = "testing"), strum_discriminants(derive(strum_macros::EnumIter)))]
// A Patricia-Merkle tree node's data.
pub enum NodeData<L: Leaf, ChildData> {
    Binary(BinaryData<ChildData>),
    Edge(EdgeData<ChildData>),
    Leaf(L),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BinaryData<ChildData> {
    pub left_data: ChildData,
    pub right_data: ChildData,
}

impl BinaryData<HashOutput> {
    pub fn flatten(&self) -> Vec<Felt> {
        vec![self.left_data.0, self.right_data.0]
    }
}

// Wraps a U256. Maximal possible value is the longest path in a tree of height 251 (2 ^ 251 - 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgePath(pub U256);

impl EdgePath {
    pub const BITS: u8 = SubTreeHeight::ACTUAL_HEIGHT.0;

    /// [EdgePath] constant that represents the longest path (from some node) in a tree.
    #[allow(clippy::as_conversions)]
    pub const MAX: Self =
        Self(U256::from_words(u128::MAX >> (U256::BITS - Self::BITS as u32), u128::MAX));

    #[cfg(any(test, feature = "testing"))]
    pub fn new_u128(value: u128) -> Self {
        let path = U256::from(value);
        Self(path)
    }
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

impl From<Felt> for EdgePath {
    fn from(value: Felt) -> Self {
        U256::from_be_bytes(value.to_bytes_be()).into()
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
    pub const MAX: Self = Self(SubTreeHeight::ACTUAL_HEIGHT.0);

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

#[allow(clippy::manual_non_exhaustive)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PathToBottom {
    pub path: EdgePath,
    pub length: EdgePathLength,
    // Used to prevent direct instantiation, while allowing destructure of other fields.
    _fake_field: (),
}

type PathToBottomResult = Result<PathToBottom, PathToBottomError>;

impl PathToBottom {
    /// Creates a new [PathToBottom] instance.
    // Asserts the path is not longer than the length.
    pub fn new(path: EdgePath, length: EdgePathLength) -> PathToBottomResult {
        let bit_length = U256::BITS - path.0.leading_zeros();
        if bit_length > u8::from(length).into() {
            return Err(PathToBottomError::MismatchedLengthError { path, length });
        }
        Ok(Self { path, length, _fake_field: () })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EdgeData<ChildData> {
    pub bottom_data: ChildData,
    pub path_to_bottom: PathToBottom,
}

impl EdgeData<HashOutput> {
    pub fn flatten(&self) -> Vec<Felt> {
        vec![
            self.path_to_bottom.length.into(),
            (&self.path_to_bottom.path).into(),
            self.bottom_data.0,
        ]
    }
}

impl PathToBottom {
    pub(crate) const LEFT_CHILD: Self =
        Self { path: EdgePath(U256::ZERO), length: EdgePathLength(1), _fake_field: () };

    pub(crate) const RIGHT_CHILD: Self =
        Self { path: EdgePath(U256::ONE), length: EdgePathLength(1), _fake_field: () };

    pub fn bottom_index(&self, root_index: NodeIndex) -> NodeIndex {
        NodeIndex::compute_bottom_index(root_index, self)
    }

    /// Returns true iff the first step on the path is to the left.
    pub fn is_left_descendant(&self) -> bool {
        self.path.0 >> (self.length.0 - 1) == 0
    }

    pub(crate) fn concat_paths(&self, other: Self) -> PathToBottom {
        Self::new(
            EdgePath::from((self.path.0 << other.length.0) + other.path.0),
            self.length + other.length,
        )
        .unwrap_or_else(|_| {
            panic!("Concatenating paths {self:?} and {other:?} unexpectedly failed.")
        })
    }

    /// Returns the path after removing the first steps (the edges from the path's origin node).
    pub fn remove_first_edges(&self, n_edges: EdgePathLength) -> PathToBottomResult {
        if self.length < n_edges {
            return Err(PathToBottomError::RemoveEdgesError { length: self.length, n_edges });
        }
        Self::new(
            EdgePath(self.path.0 & ((U256::ONE << (self.length.0 - n_edges.0)) - 1)),
            self.length - n_edges,
        )
    }

    /// Returns a path of length 0.
    pub fn new_zero() -> Self {
        Self::new(EdgePath(U256::new(0)), EdgePathLength(0))
            .expect("Creating a zero path unexpectedly failed.")
    }
}

// TODO(Ariel): Move Preimage to the fact_db module in starknet_committer (add a flatten
// trait to be implemented in starknet_committer for BinaryData and EdgeData).
#[derive(Clone, Debug, PartialEq)]
pub enum Preimage {
    Binary(BinaryData<HashOutput>),
    Edge(EdgeData<HashOutput>),
}

pub type PreimageMap = HashMap<HashOutput, Preimage>;

pub fn flatten_preimages(preimage_map: &PreimageMap) -> HashMap<HashOutput, Vec<Felt>> {
    preimage_map.iter().map(|(hash, preimage)| (*hash, preimage.flatten())).collect()
}

impl Preimage {
    pub(crate) const BINARY_LENGTH: u8 = 2;
    pub(crate) const EDGE_LENGTH: u8 = 3;

    pub fn length(&self) -> u8 {
        match self {
            Self::Binary(_) => Self::BINARY_LENGTH,
            Self::Edge(_) => Self::EDGE_LENGTH,
        }
    }

    pub fn get_binary(&self) -> Result<&BinaryData<HashOutput>, PreimageError> {
        match self {
            Self::Binary(binary) => Ok(binary),
            Self::Edge(_) => Err(PreimageError::ExpectedBinary(self.clone())),
        }
    }

    pub fn flatten(&self) -> Vec<Felt> {
        match self {
            Self::Binary(binary) => binary.flatten(),
            Self::Edge(edge) => edge.flatten(),
        }
    }
}

impl From<&MerkleNode> for Preimage {
    fn from(node: &MerkleNode) -> Self {
        match node {
            MerkleNode::BinaryNode(binary_node) => Preimage::Binary(BinaryData {
                left_data: HashOutput(binary_node.left),
                right_data: HashOutput(binary_node.right),
            }),
            MerkleNode::EdgeNode(edge_node) => {
                let length = u8::try_from(edge_node.length).unwrap_or_else(|_| {
                    panic!(
                        "EdgeNode length {} exceeds u8::MAX when converting to Preimage",
                        edge_node.length
                    )
                });
                Preimage::Edge(EdgeData {
                    bottom_data: HashOutput(edge_node.child),
                    path_to_bottom: PathToBottom::new(
                        EdgePath(U256::from_be_bytes(edge_node.path.to_bytes_be())),
                        EdgePathLength(length),
                    )
                    .unwrap_or_else(|_| {
                        panic!(
                            "Failed to create PathToBottom from MerkleNode edge: path={:?}, \
                             length={}",
                            edge_node.path, edge_node.length
                        )
                    }),
                })
            }
        }
    }
}

impl TryFrom<&Vec<Felt>> for Preimage {
    type Error = PreimageError;

    fn try_from(raw_preimage: &Vec<Felt>) -> Result<Self, Self::Error> {
        match raw_preimage.as_slice() {
            [left, right] => Ok(Preimage::Binary(BinaryData {
                left_data: HashOutput(*left),
                right_data: HashOutput(*right),
            })),
            [length, path, bottom] => {
                Ok(Preimage::Edge(EdgeData {
                    bottom_data: HashOutput(*bottom),
                    path_to_bottom: PathToBottom::new(
                        (*path).into(),
                        EdgePathLength::new((*length).try_into().map_err(|_| {
                            PreimageError::InvalidRawPreimage(raw_preimage.clone())
                        })?)?,
                    )?,
                }))
            }
            _ => Err(PreimageError::InvalidRawPreimage(raw_preimage.clone())),
        }
    }
}
