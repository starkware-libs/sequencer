use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;
use starknet_committer::db::forest_trait::ForestMetadataType;
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Error, Serialize)]
pub enum CommitterError {
    #[error("The next height to commit is {committer_offset}, got greater height {input_height}.")]
    CommitHeightHole { input_height: BlockNumber, committer_offset: BlockNumber },
    #[error("Failed to commit block number {height}: {message}")]
    Internal { height: BlockNumber, message: String },
    #[error(
        "Height {height} already committed with state diff commitment {stored_commitment}, got \
         {input_commitment}."
    )]
    InvalidStateDiffCommitment {
        input_commitment: StateDiffCommitment,
        stored_commitment: StateDiffCommitment,
        height: BlockNumber,
    },
    #[error("Failed to read metadata for {0:?}")]
    MissingMetadata(ForestMetadataType),
    #[error("State root for the committed block number {height} is missing.")]
    MissingStateRoot { height: BlockNumber },
    #[error(
        "The next height to revert is {last_committed_block}, got less height {input_height}."
    )]
    RevertHeightHole { input_height: BlockNumber, last_committed_block: BlockNumber },
}

pub type CommitterResult<T> = Result<T, CommitterError>;

#[derive(Clone, Debug, Error)]
pub enum CommitterClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    CommitterError(#[from] CommitterError),
}

pub type CommitterClientResult<T> = Result<T, CommitterClientError>;
