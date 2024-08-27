use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;

use crate::batcher_types::{ProposalContentId, StreamId};

// TODO(Tsabary/Yael/Dafna): Populate with actual errors.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error("Received proposal generation request while already generating a proposal.")]
    AlreadyGeneratingProposal,
    #[error(
        "Didn't find a proposed block for the given height {height} with the given content ID \
         {content_id}."
    )]
    ClosedBlockNotFound { height: BlockNumber, content_id: ProposalContentId },
    #[error("Internal server error.")]
    InternalError,
    #[error("Stream ID {stream_id} already exists.")]
    StreamIdAlreadyExists { stream_id: StreamId },
    #[error("Stream ID {stream_id} does not exist.")]
    StreamIdDoesNotExist { stream_id: StreamId },
    #[error("Time to deadline is out of range. Got {deadline}.")]
    TimeToDeadlineError { deadline: chrono::DateTime<Utc> },
}
