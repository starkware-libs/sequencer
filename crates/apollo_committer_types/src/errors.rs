use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Error, Serialize)]
pub enum CommitterError {
    #[error("Failed to commit block: {0}")]
    Commitment(String),
}

pub type CommitterResult<T> = Result<T, CommitterError>;

#[derive(Clone, Debug, Error)]
pub enum CommitterClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    Committer(#[from] CommitterError),
}

pub type CommitterClientResult<T> = Result<T, CommitterClientError>;
