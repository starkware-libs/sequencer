use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::batcher_types::ProposalId;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error("Internal server error.")]
    InternalError,
    #[error("Proposal with ID {proposal_id} already exists.")]
    ProposalAlreadyExists { proposal_id: ProposalId },
    #[error("Proposal with ID {proposal_id} not found.")]
    ProposalNotFound { proposal_id: ProposalId },
    #[error(
        "There is already an active proposal {}, can't start proposal {}.",
        active_proposal_id,
        new_proposal_id
    )]
    ServerBusy { active_proposal_id: ProposalId, new_proposal_id: ProposalId },
    #[error("Time to deadline is out of range. Got {deadline}.")]
    TimeToDeadlineError { deadline: chrono::DateTime<Utc> },
}
