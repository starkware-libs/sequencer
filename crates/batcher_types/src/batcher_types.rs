use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;

use crate::errors::BatcherError;

pub type StreamId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildProposalInput {
    pub stream_id: StreamId,
    pub deadline: chrono::DateTime<Utc>,
    pub height: BlockNumber,
}

impl BuildProposalInput {
    pub fn deadline_as_instant(&self) -> Result<std::time::Instant, chrono::OutOfRangeError> {
        let time_to_deadline = self.deadline - chrono::Utc::now();
        let as_duration = time_to_deadline.to_std()?;
        Ok(std::time::Instant::now() + as_duration)
    }
}

pub type BatcherResult<T> = Result<T, BatcherError>;
