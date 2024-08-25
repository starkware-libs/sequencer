use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;

use crate::errors::BatcherError;

pub type StreamId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalContentId {
    pub tx_commitment: TransactionCommitment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamContent {
    Tx(Transaction),
    StreamEnd(ProposalContentId),
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStreamContentInput {
    pub stream_id: StreamId,
}

pub type BatcherResult<T> = Result<T, BatcherError>;
