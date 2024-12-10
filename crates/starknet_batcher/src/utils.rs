use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use blockifier::abi::constants;
use chrono::Utc;
use indexmap::IndexMap;
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{BatcherResult, ProposalCommitment, ProposalStatus};
use starknet_batcher_types::errors::BatcherError;
use thiserror::Error;

use crate::block_builder::{BlockBuilderError, BlockExecutionArtifacts};

pub(crate) type ProposalResult<T> = Result<T, ProposalError>;

// Represents an error completing a proposal.
#[derive(Clone, Debug, Error)]
pub(crate) enum ProposalError {
    #[error(transparent)]
    BlockBuilderError(Arc<BlockBuilderError>),
    #[error("Proposal was aborted")]
    Aborted,
}

impl From<ProposalError> for BatcherError {
    fn from(err: ProposalError) -> Self {
        match err {
            ProposalError::BlockBuilderError(..) => BatcherError::InternalError,
            ProposalError::Aborted => BatcherError::ProposalAborted,
        }
    }
}

// Represents a spawned task of building new block proposal.
pub(crate) struct ProposalTask {
    pub abort_signal_sender: tokio::sync::oneshot::Sender<()>,
    pub join_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct ProposalOutput {
    pub state_diff: ThinStateDiff,
    pub commitment: ProposalCommitment,
    pub tx_hashes: HashSet<TransactionHash>,
    pub nonces: HashMap<ContractAddress, Nonce>,
}

impl From<BlockExecutionArtifacts> for ProposalOutput {
    fn from(artifacts: BlockExecutionArtifacts) -> Self {
        let commitment_state_diff = artifacts.commitment_state_diff;
        let nonces = HashMap::from_iter(
            commitment_state_diff
                .address_to_nonce
                .iter()
                .map(|(address, nonce)| (*address, *nonce)),
        );

        // TODO: Get these from the transactions.
        let deployed_contracts = IndexMap::new();
        let declared_classes = IndexMap::new();
        let state_diff = ThinStateDiff {
            deployed_contracts,
            storage_diffs: commitment_state_diff.storage_updates,
            declared_classes,
            nonces: commitment_state_diff.address_to_nonce,
            // TODO: Remove this when the structure of storage diffs changes.
            deprecated_declared_classes: Vec::new(),
            replaced_classes: IndexMap::new(),
        };
        let commitment =
            ProposalCommitment { state_diff_commitment: calculate_state_diff_hash(&state_diff) };
        let tx_hashes = HashSet::from_iter(artifacts.execution_infos.keys().copied());

        Self { state_diff, commitment, tx_hashes, nonces }
    }
}

pub(crate) fn deadline_as_instant(
    deadline: chrono::DateTime<Utc>,
) -> BatcherResult<tokio::time::Instant> {
    let time_to_deadline = deadline - chrono::Utc::now();
    let as_duration =
        time_to_deadline.to_std().map_err(|_| BatcherError::TimeToDeadlineError { deadline })?;
    Ok((std::time::Instant::now() + as_duration).into())
}

pub(crate) fn verify_block_input(
    height: BlockNumber,
    block_number: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    verify_non_empty_retrospective_block_hash(height, retrospective_block_hash)?;
    verify_block_number(height, block_number)?;
    Ok(())
}

pub(crate) fn verify_non_empty_retrospective_block_hash(
    height: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    if height >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)
        && retrospective_block_hash.is_none()
    {
        return Err(BatcherError::MissingRetrospectiveBlockHash);
    }
    Ok(())
}

pub(crate) fn verify_block_number(
    height: BlockNumber,
    block_number: BlockNumber,
) -> BatcherResult<()> {
    if block_number != height {
        return Err(BatcherError::InvalidBlockNumber { active_height: height, block_number });
    }
    Ok(())
}

// Return the appropriate ProposalStatus for a given ProposalError.
pub(crate) fn proposal_status_from(proposal_error: ProposalError) -> BatcherResult<ProposalStatus> {
    match proposal_error {
        ProposalError::BlockBuilderError(err) => {
            if let BlockBuilderError::FailOnError(_) = err.as_ref() {
                // The proposal either failed due to bad input (e.g. invalid transactions), or
                // couldn't finish in time.
                Ok(ProposalStatus::InvalidProposal)
            } else {
                Err(BatcherError::InternalError)
            }
        }
        ProposalError::Aborted => Err(BatcherError::ProposalAborted),
    }
}
