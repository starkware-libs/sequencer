use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CommitmentSyncError {
    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type CommitmentSyncResult<T> = Result<T, CommitmentSyncError>;

#[derive(Debug, Error)]
pub enum CommitmentSyncClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    CommitmentSyncError(#[from] CommitmentSyncError),
}
