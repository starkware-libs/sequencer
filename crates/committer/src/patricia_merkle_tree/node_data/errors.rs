use std::fmt::Debug;

use thiserror::Error;

use crate::patricia_merkle_tree::node_data::inner_node::{EdgePath, EdgePathLength};
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
    #[error("Missing modification data at index {0:?}.")]
    MissingLeafModificationData(NodeIndex),
}

pub type LeafResult<T> = Result<T, LeafError>;
