use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::batcher_types::StreamId;

// TODO(Tsabary/Yael/Dafna): Populate with actual errors.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatcherError {
    #[error("Received proposal generation request while already generating a proposal.")]
    AlreadyGeneratingProposal,
    #[error("Internal server error.")]
    InternalError,
    #[error("Stream ID {stream_id} already exists.")]
    StreamIdAlreadyExists { stream_id: StreamId },
    #[error("Stream ID {stream_id} does not exist.")]
    StreamIdDoesNotExist { stream_id: StreamId },
    #[error("Time to deadline is out of range. Got {deadline}.")]
    TimeToDeadlineError { deadline: chrono::DateTime<Utc> },
}
