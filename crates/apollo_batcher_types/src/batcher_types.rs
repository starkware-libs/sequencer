use std::fmt::Debug;

use blockifier::blockifier::transaction_executor::CompiledClassHashesToMigrate;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use chrono::prelude::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockInfo, BlockNumber};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::StateDiffCommitment;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::errors::BatcherError;

// TODO(Matan): decide on the id structure
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

pub type Round = u32;

#[derive(Clone, Debug, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalCommitment {
    pub state_diff_commitment: StateDiffCommitment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposeBlockInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
    pub block_info: BlockInfo,
    pub proposal_round: Round,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetProposalContentInput {
    // TBD: We don't really need the proposal_id because there is only one proposal at a time.
    pub proposal_id: ProposalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetHeightResponse {
    pub height: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetProposalContentResponse {
    pub content: GetProposalContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GetProposalContent {
    Txs(Vec<InternalConsensusTransaction>),
    Finished { id: ProposalCommitment, final_n_executed_txs: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
// TODO(Dan): Consider unifying with BuildProposalInput as they have the same fields.
pub struct ValidateBlockInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
    pub block_info: BlockInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendProposalContentInput {
    pub proposal_id: ProposalId,
    pub content: SendProposalContent,
}

/// The content of the stream that the consensus sends to the batcher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SendProposalContent {
    Txs(Vec<InternalConsensusTransaction>),
    /// Contains the final number of transactions in the block.
    Finish(usize),
    Abort,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SendProposalContentResponse {
    pub response: ProposalStatus,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
pub struct CentralObjects {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub bouncer_weights: BouncerWeights,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
    pub compiled_class_hashes_to_migrate: CompiledClassHashesToMigrate,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
pub struct DecisionReachedResponse {
    // TODO(Yael): Consider passing the state_diff as CommitmentStateDiff inside CentralObjects.
    // Today the ThinStateDiff is used for the state sync but it may not be needed in the future.
    pub state_diff: ThinStateDiff,
    pub l2_gas_used: GasAmount,
    pub central_objects: CentralObjects,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
    Processing,
    // Only sent in response to `Finish`.
    Finished(ProposalCommitment),
    // Only sent in response to `Abort`.
    Aborted,
    // May be caused due to handling of a previous item of the new proposal.
    // In this case, the proposal is aborted and no additional content will be processed.
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RevertBlockInput {
    pub height: BlockNumber,
}

pub type BatcherResult<T> = Result<T, BatcherError>;
