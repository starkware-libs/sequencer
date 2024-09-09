use chrono::prelude::*;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;

use crate::errors::BatcherError;

// TBD: Do we need id from the client?
pub type ProposalId = u64;

#[derive(Clone, Copy, Debug, Default, Display, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalContentId {
    pub tx_commitment: TransactionCommitment,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StreamContent {
    Txs(Vec<Transaction>),
    Fin(ProposalContentId),
    Abort,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildProposalInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStreamContentInput {
    // TBD: We don't really need the proposal_id because there is only one proposal at a time.
    pub proposal_id: ProposalId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidateProposalInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendStreamContentInput {
    pub proposal_id: ProposalId,
    pub content: StreamContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendContentResponse {
    Ack,
    ValidProposal(ProposalContentId),
    InvalidProposal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartHeightInput {
    pub height: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionReachedInput {
    // TBD: We don't both ids.
    pub proposal_id: ProposalId,
    pub content_id: ProposalContentId,
}

pub type BatcherResult<T> = Result<T, BatcherError>;
