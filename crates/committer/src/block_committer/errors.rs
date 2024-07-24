use thiserror::Error;

use crate::forest_errors::ForestError;

#[derive(Debug, Error)]
pub enum BlockCommitmentError {
    #[error(transparent)]
    ForestError(#[from] ForestError),
}
