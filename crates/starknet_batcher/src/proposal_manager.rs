use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{ProposalCommitment, ProposalId};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, Instrument};

use crate::block_builder::{BlockBuilderError, BlockBuilderTrait, BlockExecutionArtifacts};

#[derive(Debug, Error)]
pub enum GenerateProposalError {
    #[error(
        "Received proposal generation request with id {new_proposal_id} while already generating \
         proposal with id {current_generating_proposal_id}."
    )]
    AlreadyGeneratingProposal {
        current_generating_proposal_id: ProposalId,
        new_proposal_id: ProposalId,
    },
    #[error(transparent)]
    BlockBuilderError(#[from] BlockBuilderError),
    #[error("No active height to work on.")]
    NoActiveHeight,
    #[error("Proposal with id {proposal_id} already exists.")]
    ProposalAlreadyExists { proposal_id: ProposalId },
}

#[derive(Clone, Debug, Error)]
pub enum ProposalError {
    #[error(transparent)]
    BlockBuilderError(Arc<BlockBuilderError>),
    #[error("Proposal was aborted")]
    Aborted,
}

pub(crate) enum InternalProposalStatus {
    Processing,
    Finished,
    Failed,
    NotFound,
}

#[async_trait]
pub trait ProposalManagerTrait: Send + Sync {
    async fn spawn_proposal(
        &mut self,
        proposal_id: ProposalId,
        mut block_builder: Box<dyn BlockBuilderTrait>,
        abort_signal_sender: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(), GenerateProposalError>;

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<ProposalOutput>>;

    #[allow(dead_code)]
    async fn get_active_proposal(&self) -> Option<ProposalId>;

    #[allow(dead_code)]
    async fn get_completed_proposals(
        &self,
    ) -> Arc<Mutex<HashMap<ProposalId, ProposalResult<ProposalOutput>>>>;

    async fn await_active_proposal(&mut self) -> bool;

    async fn get_proposal_status(&self, proposal_id: ProposalId) -> InternalProposalStatus;

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<ProposalCommitment>>;

    async fn abort_proposal(&mut self, proposal_id: ProposalId);

    // Resets the proposal manager, aborting any active proposal.
    async fn reset(&mut self);
}

// Represents a spawned task of building new block proposal.
struct ProposalTask {
    abort_signal_sender: tokio::sync::oneshot::Sender<()>,
    join_handle: tokio::task::JoinHandle<()>,
}

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Committing accepted proposals to the storage.
///
/// Triggered by the consensus.
pub(crate) struct ProposalManager {
    /// The block proposal that is currently being built, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_task: Option<ProposalTask>,

    executed_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult<ProposalOutput>>>>,
}

pub type ProposalResult<T> = Result<T, ProposalError>;

#[derive(Debug, PartialEq)]
pub struct ProposalOutput {
    pub state_diff: ThinStateDiff,
    pub commitment: ProposalCommitment,
    pub tx_hashes: HashSet<TransactionHash>,
    pub nonces: HashMap<ContractAddress, Nonce>,
}

#[async_trait]
impl ProposalManagerTrait for ProposalManager {
    /// Starts a new block proposal generation task for the given proposal_id.
    /// Uses the given block_builder to generate the proposal.
    #[instrument(skip(self, block_builder), err)]
    async fn spawn_proposal(
        &mut self,
        proposal_id: ProposalId,
        mut block_builder: Box<dyn BlockBuilderTrait>,
        abort_signal_sender: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(), GenerateProposalError> {
        self.set_active_proposal(proposal_id).await?;

        info!("Starting generation of a new proposal with id {}.", proposal_id);

        let active_proposal = self.active_proposal.clone();
        let executed_proposals = self.executed_proposals.clone();

        let join_handle = tokio::spawn(
            async move {
                let result = block_builder
                    .build_block()
                    .await
                    .map(ProposalOutput::from)
                    .map_err(|e| ProposalError::BlockBuilderError(Arc::new(e)));

                // The proposal is done, clear the active proposal.
                // Keep the proposal result only if it is the same as the active proposal.
                // The active proposal might have changed if this proposal was aborted.
                let mut active_proposal = active_proposal.lock().await;
                if *active_proposal == Some(proposal_id) {
                    active_proposal.take();
                    executed_proposals.lock().await.insert(proposal_id, result);
                }
            }
            .in_current_span(),
        );

        self.active_proposal_task = Some(ProposalTask { abort_signal_sender, join_handle });
        Ok(())
    }

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<ProposalOutput>> {
        self.executed_proposals.lock().await.remove(&proposal_id)
    }

    async fn get_active_proposal(&self) -> Option<ProposalId> {
        *self.active_proposal.lock().await
    }

    async fn get_completed_proposals(
        &self,
    ) -> Arc<Mutex<HashMap<ProposalId, ProposalResult<ProposalOutput>>>> {
        self.executed_proposals.clone()
    }

    // Awaits the active proposal.
    // Returns true if there was an active proposal, and false otherwise.
    async fn await_active_proposal(&mut self) -> bool {
        if let Some(proposal_task) = self.active_proposal_task.take() {
            proposal_task.join_handle.await.ok();
            return true;
        }
        false
    }

    // Returns None if the proposal does not exist, otherwise, returns the status of the proposal.
    async fn get_proposal_status(&self, proposal_id: ProposalId) -> InternalProposalStatus {
        match self.executed_proposals.lock().await.get(&proposal_id) {
            Some(Ok(_)) => InternalProposalStatus::Finished,
            Some(Err(_)) => InternalProposalStatus::Failed,
            None => {
                if self.active_proposal.lock().await.as_ref() == Some(&proposal_id) {
                    InternalProposalStatus::Processing
                } else {
                    InternalProposalStatus::NotFound
                }
            }
        }
    }

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<ProposalCommitment>> {
        if *self.active_proposal.lock().await == Some(proposal_id) {
            self.await_active_proposal().await;
        }
        let proposals = self.executed_proposals.lock().await;
        let output = proposals.get(&proposal_id);
        match output {
            Some(Ok(output)) => Some(Ok(output.commitment)),
            Some(Err(e)) => Some(Err(e.clone())),
            None => None,
        }
    }

    // Aborts the proposal with the given ID, if active.
    // Should be used in validate flow, if the consensus decides to abort the proposal.
    async fn abort_proposal(&mut self, proposal_id: ProposalId) {
        if *self.active_proposal.lock().await == Some(proposal_id) {
            self.abort_active_proposal().await;
            self.executed_proposals.lock().await.insert(proposal_id, Err(ProposalError::Aborted));
        }
    }

    async fn reset(&mut self) {
        self.abort_active_proposal().await;
        self.executed_proposals.lock().await.clear();
    }
}

impl ProposalManager {
    pub fn new() -> Self {
        Self {
            active_proposal: Arc::new(Mutex::new(None)),
            active_proposal_task: None,
            executed_proposals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Sets a new active proposal task.
    // Fails if either there is no active height, there is another proposal being generated, or a
    // proposal with the same ID already exists.
    async fn set_active_proposal(
        &mut self,
        proposal_id: ProposalId,
    ) -> Result<(), GenerateProposalError> {
        if self.executed_proposals.lock().await.contains_key(&proposal_id) {
            return Err(GenerateProposalError::ProposalAlreadyExists { proposal_id });
        }

        let mut active_proposal = self.active_proposal.lock().await;
        if let Some(current_generating_proposal_id) = *active_proposal {
            return Err(GenerateProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id: proposal_id,
            });
        }

        debug!("Set proposal {} as the one being generated.", proposal_id);
        *active_proposal = Some(proposal_id);
        Ok(())
    }

    // Ends the current active proposal.
    // This call is non-blocking.
    async fn abort_active_proposal(&mut self) {
        self.active_proposal.lock().await.take();
        if let Some(proposal_task) = self.active_proposal_task.take() {
            proposal_task.abort_signal_sender.send(()).ok();
        }
    }
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
