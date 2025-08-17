use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CommitterError {
    #[error("Block commitment error: {0:?}")]
    BlockCommitment(String),
    #[error("Block {0} is already being committed.")]
    BlockAlreadyCommitted(BlockNumber),
    #[error("Error joining the commitment task: {0}")]
    Join(String),
}

pub type CommitterResult<T> = Result<T, CommitterError>;

#[derive(Debug, Error)]
pub enum CommitterClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    CommitterError(#[from] CommitterError),
}
