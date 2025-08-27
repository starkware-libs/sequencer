use std::fmt::Debug;

use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength, Preimage};
use crate::patricia_merkle_tree::types::NodeIndex;

#[derive(Debug, Error)]
pub enum PathToBottomError {
    #[error("Tried to remove {n_edges:?} edges from a {length:?} length path.")]
    RemoveEdgesError { length: EdgePathLength, n_edges: EdgePathLength },
    #[error("EdgePath {path:?} is too long for EdgePathLength {length:?}.")]
    MismatchedLengthError { path: EdgePath, length: EdgePathLength },
}

#[derive(Debug, Error)]
pub enum EdgePathError {
    #[error("Length {length:?} is not in range [0, EdgePathLength::MAX]")]
    IllegalLength { length: u8 },
}

#[derive(Debug, Error)]
pub enum LeafError {
    #[error("Got the following error when trying to compute the leaf: {0:?}")]
    LeafComputationError(String),
    #[error("Missing modification data at index {0:?}.")]
    MissingLeafModificationData(NodeIndex),
}

pub type LeafResult<T> = Result<T, LeafError>;

#[derive(Debug, Error)]
pub enum PreimageError {
    #[error(transparent)]
    EdgePath(#[from] EdgePathError),
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Invalid raw preimage: {0:?}, length should be 2 or 3.")]
    InvalidRawPreimage(Vec<Felt>),
    #[error(transparent)]
    PathToBottom(#[from] PathToBottomError),
}
