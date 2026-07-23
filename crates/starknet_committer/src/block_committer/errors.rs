use starknet_patricia::patricia_merkle_tree::traversal::TraversalError;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::errors::SerializationError;
use thiserror::Error;

use crate::forest::forest_errors::ForestError;

#[derive(Debug, Error)]
pub enum BlockCommitmentError {
    #[error(transparent)]
    ForestError(#[from] ForestError),
    #[error(transparent)]
    Traversal(#[from] TraversalError),
}

#[cfg(feature = "os_input")]
#[derive(Debug, Error)]
pub enum CommitBlockWithWitnessesError {
    #[error(transparent)]
    BlockCommitment(#[from] BlockCommitmentError),
    #[error("pre-commit witness paths: {0:?}")]
    PreCommitWitnessFetch(TraversalError),
    #[error("post-commit witness paths: {0:?}")]
    PostCommitWitnessFetch(TraversalError),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
}
