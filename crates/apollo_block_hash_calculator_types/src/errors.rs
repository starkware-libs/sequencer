use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum BlockHashCalculatorError {
    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type BlockHashCalculatorResult<T> = Result<T, BlockHashCalculatorError>;

#[derive(Debug, Error)]
pub enum BlockHashCalculatorClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    BlockHashCalculatorError(#[from] BlockHashCalculatorError),
}
