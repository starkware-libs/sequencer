use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
pub use starknet_consensus_manager_types::consensus_manager_types::ProposalId;

use crate::errors::BatcherError;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalContentId {
    pub tx_commitment: TransactionCommitment,
    pub state_diff: ThinStateDiff,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildProposalInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub block_hash_10_blocks_ago: BlockHash,
    // TODO: Should we get the gas price here?
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStreamContentInput {
    // TBD: We don't really need the proposal_id because there is only one proposal at a time.
    pub proposal_id: ProposalId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GetStreamContentResponse {
    Txs(Vec<Transaction>),
    Finished(ProposalContentId),
    // Indicates an error with the content, not the components (i.e., batcher/consensus).
    Abort,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidateProposalInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendStreamContentInput {
    pub proposal_id: ProposalId,
    pub content: SendStreamContent,
}

/// The content of the stream that the consensus sends to the batcher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SendStreamContent {
    Txs(Vec<Transaction>),
    Finish,
    Abort,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendContentResponse {
    Ack,
    // Only sent in response to `Fin`
    Finished(ProposalContentId),
    // May be caused due to handling of a previous item on the stream.
    InvalidProposal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartHeightInput {
    pub height: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionReachedInput {
    pub proposal_id: ProposalId,
}

pub type BatcherResult<T> = Result<T, BatcherError>;
