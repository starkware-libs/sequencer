use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;

use crate::batcher_types::ProposalId;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error(
        "Decision reached for proposal with ID {proposal_id} that does not exist (might still \
         being executed)."
    )]
    ExecutedProposalNotFound { proposal_id: ProposalId },
    #[error(
        "Height {storage_height} already passed, can't start working on height {requested_height}."
    )]
    HeightAlreadyPassed { storage_height: BlockNumber, requested_height: BlockNumber },
    #[error("Internal server error.")]
    InternalError,
    #[error("Attempt to start proposal with no active height.")]
    NoActiveHeight,
    #[error(
        "There is already an active proposal {}, can't start proposal {}.",
        active_proposal_id,
        new_proposal_id
    )]
    ServerBusy { active_proposal_id: ProposalId, new_proposal_id: ProposalId },
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
        "Storage is not synced. Storage height: {storage_height}, requested height: \
         {requested_height}."
    )]
    StorageNotSynced { storage_height: BlockNumber, requested_height: BlockNumber },
    #[error("Time to deadline is out of range. Got {deadline}.")]
    TimeToDeadlineError { deadline: chrono::DateTime<Utc> },
}
