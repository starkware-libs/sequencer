use std::sync::Arc;

use apollo_batcher_types::batcher_types::{BatcherResult, ProposalStatus};
use apollo_batcher_types::errors::BatcherError;
use blockifier::abi::constants;
use chrono::Utc;
use starknet_api::block::{BlockHashAndNumber, BlockNumber};

use crate::block_builder::BlockBuilderError;

// BlockBuilderError is wrapped in an Arc since it doesn't implement Clone.
pub(crate) type ProposalResult<T> = Result<T, Arc<BlockBuilderError>>;

// Represents a spawned task of building new block proposal.
pub(crate) struct ProposalTask {
    pub abort_signal_sender: tokio::sync::oneshot::Sender<()>,
    pub final_n_executed_txs_sender: Option<tokio::sync::oneshot::Sender<usize>>,
    // Handle for awaiting completion of the block proposal execution task.
    pub execution_join_handle: tokio::task::JoinHandle<()>,
    // Optional handle for awaiting completion of the pre-confirmed block writer task,
    // which streams transaction execution states to Cende during block construction.
    pub writer_join_handle: Option<tokio::task::JoinHandle<()>>,
}

pub(crate) fn deadline_as_instant(
    deadline: chrono::DateTime<Utc>,
) -> BatcherResult<tokio::time::Instant> {
    let time_to_deadline = deadline - chrono::Utc::now();
    let as_duration =
        time_to_deadline.to_std().map_err(|_| BatcherError::TimeToDeadlineError { deadline })?;
    Ok((std::time::Instant::now() + as_duration).into())
}

pub(crate) fn verify_block_input(
    height: BlockNumber,
    block_number: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    verify_non_empty_retrospective_block_hash(height, retrospective_block_hash)?;
    verify_block_number(height, block_number)?;
    Ok(())
}

pub(crate) fn verify_non_empty_retrospective_block_hash(
    height: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    if height >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)
        && retrospective_block_hash.is_none()
    {
        return Err(BatcherError::MissingRetrospectiveBlockHash);
    }
    Ok(())
}

pub(crate) fn verify_block_number(
    height: BlockNumber,
    block_number: BlockNumber,
) -> BatcherResult<()> {
    if block_number != height {
        return Err(BatcherError::InvalidBlockNumber { active_height: height, block_number });
    }
    Ok(())
}

// Return the appropriate ProposalStatus for a given ProposalError.
pub(crate) fn proposal_status_from(
    block_builder_error: Arc<BlockBuilderError>,
) -> BatcherResult<ProposalStatus> {
    match block_builder_error.as_ref() {
        // FailOnError means the proposal either failed due to bad input (e.g. invalid
        // transactions), or couldn't finish in time.
        BlockBuilderError::FailOnError(err) => Ok(ProposalStatus::InvalidProposal(err.to_string())),
        BlockBuilderError::Aborted => Err(BatcherError::ProposalAborted),
        _ => {
            tracing::error!("Unexpected error: {}", block_builder_error);
            Err(BatcherError::InternalError)
        }
    }
}
