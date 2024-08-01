use thiserror::Error;

use crate::forest::forest_errors::ForestError;

#[derive(Debug, Error)]
pub enum BlockCommitmentError {
    #[error(transparent)]
    ForestError(#[from] ForestError),
}
