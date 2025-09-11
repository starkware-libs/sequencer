use starknet_patricia::patricia_merkle_tree::traversal::TraversalError;
use thiserror::Error;

use crate::forest::forest_errors::ForestError;

#[derive(Debug, Error)]
pub enum BlockCommitmentError {
    #[error(transparent)]
    ForestError(#[from] ForestError),
    #[error(transparent)]
    Traversal(#[from] TraversalError),
}
