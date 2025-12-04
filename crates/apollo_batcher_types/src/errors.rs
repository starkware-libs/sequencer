use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;

use crate::batcher_types::ProposalId;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error(
        "There is already an active proposal {}, can't start proposal {}.",
        active_proposal_id,
        new_proposal_id
    )]
    AnotherProposalInProgress { active_proposal_id: ProposalId, new_proposal_id: ProposalId },
    #[error(
        "Decision reached for proposal with ID {proposal_id} that does not exist (might still \
         being executed)."
    )]
    ExecutedProposalNotFound { proposal_id: ProposalId },
    #[error("Height is in progress.")]
    HeightInProgress,
    #[error("Internal server error: {0}")]
    InternalError(String),
    #[error("Invalid block number. The active height is {active_height}, got {block_number}.")]
    InvalidBlockNumber { active_height: BlockNumber, block_number: BlockNumber },
    #[error("Missing retrospective block hash.")]
    MissingRetrospectiveBlockHash,
    #[error("Attempt to start proposal with no active height.")]
    NoActiveHeight,
    #[error("Not ready to begin work on proposal.")]
    NotReady,
    #[error("Proposal aborted.")]
    ProposalAborted,
    #[error("Proposal with ID {proposal_id} already exists.")]
    ProposalAlreadyExists { proposal_id: ProposalId },
    #[error(
        "Proposal with ID {proposal_id} is already done processing and cannot get more \
         transactions."
    )]
    ProposalAlreadyFinished { proposal_id: ProposalId },
    #[error("Proposal failed.")]
    ProposalFailed,
    #[error("Proposal with ID {proposal_id} not found.")]
    ProposalNotFound { proposal_id: ProposalId },
    #[error(
        "Storage height marker mismatch. Storage marker (first unwritten height): \
         {marker_height}, requested height: {requested_height}."
    )]
    StorageHeightMarkerMismatch { marker_height: BlockNumber, requested_height: BlockNumber },
    #[error("Time to deadline is out of range. Got {deadline}.")]
    TimeToDeadlineError { deadline: chrono::DateTime<Utc> },
}
