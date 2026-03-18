use std::fmt::Debug;
use std::ops::Deref;

use blockifier::blockifier::transaction_executor::CompiledClassHashesForMigration;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use chrono::prelude::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockInfo, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::{BlockHeaderCommitments, PartialBlockHash};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

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

/// Commitment identifying a proposed block (its partial block hash).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalCommitment {
    pub partial_block_hash: PartialBlockHash,
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

/// Artifact-derived fields for a finished proposal. Use with
/// [`FinishedProposalInfo::new`] to build the full info.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinishedProposalInfoWithoutParent {
    pub proposal_commitment: ProposalCommitment,
    pub final_n_executed_txs: usize,
    pub block_header_commitments: BlockHeaderCommitments,
    pub l2_gas_used: GasAmount,
}

/// Information returned when block building has finished (proposer or validator).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinishedProposalInfo {
    #[serde(flatten)]
    pub artifact: FinishedProposalInfoWithoutParent,
    // None for the first block
    pub parent_proposal_commitment: Option<ProposalCommitment>,
}

impl Deref for FinishedProposalInfo {
    type Target = FinishedProposalInfoWithoutParent;

    fn deref(&self) -> &Self::Target {
        &self.artifact
    }
}

impl FinishedProposalInfo {
    /// Builds [`FinishedProposalInfo`] from artifact-derived fields and the parent commitment.
    pub fn new(
        artifact_derived: FinishedProposalInfoWithoutParent,
        parent_proposal_commitment: Option<ProposalCommitment>,
    ) -> Self {
        Self { artifact: artifact_derived, parent_proposal_commitment }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GetProposalContent {
    Txs(Vec<InternalConsensusTransaction>),
    Finished(FinishedProposalInfo),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidateBlockInput {
    pub proposal_id: ProposalId,
    pub deadline: chrono::DateTime<Utc>,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
    pub block_info: BlockInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendTxsForProposalInput {
    pub proposal_id: ProposalId,
    pub txs: Vec<InternalConsensusTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinishProposalInput {
    pub proposal_id: ProposalId,
    pub final_n_executed_txs: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SendTxsForProposalStatus {
    Processing,
    // May be caused due to handling of a previous item of the new proposal.
    // In this case, the proposal is aborted and no additional content will be processed.
    InvalidProposal(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FinishProposalStatus {
    Finished(FinishedProposalInfo),
    // May be caused due to handling of a previous item of the new proposal.
    // In this case, the proposal is aborted and no additional content will be processed.
    InvalidProposal(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
pub struct CentralObjects {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub bouncer_weights: BouncerWeights,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
    pub compiled_class_hashes_for_migration: CompiledClassHashesForMigration,
    pub parent_proposal_commitment: Option<ProposalCommitment>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
pub struct DecisionReachedResponse {
    // TODO(Yael): Consider passing the state_diff as CommitmentStateDiff inside CentralObjects.
    // Today the ThinStateDiff is used for the state sync but it may not be needed in the future.
    pub state_diff: ThinStateDiff,
    pub central_objects: CentralObjects,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProposalStatus {
    Processing,
    // May be caused due to handling of a previous item of the new proposal.
    // In this case, the proposal is aborted and no additional content will be processed.
    InvalidProposal(String),
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

/// Input for executing a view (read-only) entry point on a contract against the latest committed
/// batcher state.
#[derive(Debug, Serialize, Deserialize)]
pub struct CallContractInput {
    pub contract_address: ContractAddress,
    pub entry_point: String,
    pub calldata: Vec<Felt>,
}

/// Output of a successful view entry point call.
#[derive(Debug, Serialize, Deserialize)]
pub struct CallContractOutput {
    pub retdata: Vec<Felt>,
}

pub type BatcherResult<T> = Result<T, BatcherError>;
