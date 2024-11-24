use std::fmt::Debug;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockNumber, BlockTimestamp, GasPriceVector};
use starknet_api::core::{ContractAddress, StateDiffCommitment};
use starknet_api::executable_transaction::Transaction;

use crate::errors::BatcherError;

// TODO (Matan) decide on the id structure
#[derive(
    Copy,
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    derive_more::Display,
    Hash,
)]
pub struct ProposalId(pub u64);

#[derive(Clone, Debug, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalCommitment {
    pub state_diff_commitment: StateDiffCommitment,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
/// This struct is a subset of `BlockInfo`, used by the blockifier. The member `block_number` is
/// called `height` in the consensus-batcher context. It is passed to the batcher during a previous
/// stage of the process.
pub struct ThinBlockInfo {
    pub block_timestamp: BlockTimestamp,

    // Fee-related.
    pub sequencer_address: ContractAddress,
    // TODO(Arni): Align with `GasPrices` in `BlockInfo`.
    pub eth_gas_prices: GasPriceVector,
    pub strk_gas_prices: GasPriceVector,
    pub use_kzg_da: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposeBlockInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
    // TODO: Fill thin block info.
    pub thin_block_info: ThinBlockInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetProposalContentInput {
    // TBD: We don't really need the proposal_id because there is only one proposal at a time.
    pub proposal_id: ProposalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetProposalContentResponse {
    pub content: GetProposalContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GetProposalContent {
    Txs(Vec<Transaction>),
    Finished(ProposalCommitment),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
// TODO(Dan): Consider unifying with BuildProposalInput as they have the same fields.
pub struct ValidateBlockInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
    // TODO: Fill thin block info.
    pub thin_block_info: ThinBlockInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendProposalContentInput {
    pub proposal_id: ProposalId,
    pub content: SendProposalContent,
}

/// The content of the stream that the consensus sends to the batcher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SendProposalContent {
    Txs(Vec<Transaction>),
    Finish,
    Abort,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SendProposalContentResponse {
    pub response: ProposalStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
    Processing,
    // Only sent in response to `Finish`.
    Finished(ProposalCommitment),
    // Only sent in response to `Abort`.
    Aborted,
    // May be caused due to handling of a previous item of the new proposal.
    // In this case, the propsal is aborted and no additional content will be processed.
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
