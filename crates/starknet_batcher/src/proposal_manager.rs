use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{ProposalCommitment, ProposalId};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, Instrument};

use crate::batcher::BatcherStorageReaderTrait;
use crate::block_builder::{
    BlockBuilderError,
    BlockBuilderFactoryTrait,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
};
use crate::transaction_provider::ProposeTransactionProvider;

#[derive(Debug, Error)]
pub enum StartHeightError {
    #[error(
        "Requested height {requested_height} is lower than the current storage height \
         {storage_height}."
    )]
    HeightAlreadyPassed { storage_height: BlockNumber, requested_height: BlockNumber },
    #[error(transparent)]
    StorageError(#[from] papyrus_storage::StorageError),
    #[error(
        "Storage is not synced. Storage height: {storage_height}, requested height: \
         {requested_height}."
    )]
    StorageNotSynced { storage_height: BlockNumber, requested_height: BlockNumber },
}

#[derive(Debug, Error)]
pub enum BuildProposalError {
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
pub enum GetProposalResultError {
    #[error(transparent)]
    BlockBuilderError(Arc<BlockBuilderError>),
    #[error("Proposal with id {proposal_id} does not exist.")]
    ProposalDoesNotExist { proposal_id: ProposalId },
}

pub enum ProposalStatus {
    Processing,
    Finished,
    Failed,
    NotFound,
}

#[async_trait]
pub trait ProposalManagerTrait: Send + Sync {
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError>;

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), BuildProposalError>;

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalOutput>;

    async fn get_proposal_status(&self, proposal_id: ProposalId) -> ProposalStatus;

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalCommitment>;
}

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
pub(crate) struct ProposalManager {
    storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    active_height: Option<BlockNumber>,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_handle: Option<ActiveTaskHandle>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait + Send + Sync>,
    executed_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult<ProposalOutput>>>>,
}

type ActiveTaskHandle = tokio::task::JoinHandle<()>;
pub type ProposalResult<T> = Result<T, GetProposalResultError>;

#[derive(Debug, PartialEq)]
pub struct ProposalOutput {
    pub state_diff: ThinStateDiff,
    pub commitment: ProposalCommitment,
    pub tx_hashes: HashSet<TransactionHash>,
    pub nonces: HashMap<ContractAddress, Nonce>,
}

#[async_trait]
impl ProposalManagerTrait for ProposalManager {
    /// Starts working on the given height.
    #[instrument(skip(self), err)]
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError> {
        self.reset_active_height().await;

        let next_height = self.storage_reader.height()?;
        if next_height < height {
            error!(
                "Storage is not synced. Storage height: {}, requested height: {}.",
                next_height, height
            );
            return Err(StartHeightError::StorageNotSynced {
                storage_height: next_height,
                requested_height: height,
            });
        }
        if next_height > height {
            return Err(StartHeightError::HeightAlreadyPassed {
                storage_height: next_height,
                requested_height: height,
            });
        }
        info!("Starting to work on height {}.", height);
        self.active_height = Some(height);
        Ok(())
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    /// Requires tx_sender for sending the generated transactions to the caller.
    #[instrument(skip(self, tx_sender, tx_provider), err, fields(self.active_height))]
    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), BuildProposalError> {
        self.set_active_proposal(proposal_id).await?;

        info!("Starting generation of a new proposal with id {}.", proposal_id);

        let height = self.active_height.expect("No active height.");

        let block_builder = self.block_builder_factory.create_block_builder(
            height,
            retrospective_block_hash,
            deadline,
            Box::new(tx_provider),
            Some(tx_sender.clone()),
            false,
        )?;

        self.spawn_build_block_task(proposal_id, block_builder).await;

        Ok(())
    }

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalOutput> {
        self.executed_proposals
            .lock()
            .await
            .remove(&proposal_id)
            .ok_or(GetProposalResultError::ProposalDoesNotExist { proposal_id })?
    }

    // Returns None if the proposal does not exist, otherwise, returns the status of the proposal.
    async fn get_proposal_status(&self, proposal_id: ProposalId) -> ProposalStatus {
        match self.executed_proposals.lock().await.get(&proposal_id) {
            Some(Ok(_)) => ProposalStatus::Finished,
            Some(Err(_)) => ProposalStatus::Failed,
            None => {
                if self.active_proposal.lock().await.as_ref() == Some(&proposal_id) {
                    ProposalStatus::Processing
                } else {
                    ProposalStatus::NotFound
                }
            }
        }
    }

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalCommitment> {
        self.await_proposal_completion(proposal_id).await;
        let proposals = self.executed_proposals.lock().await;
        let output = proposals
            .get(&proposal_id)
            .ok_or(GetProposalResultError::ProposalDoesNotExist { proposal_id })?;
        match output {
            Ok(output) => Ok(output.commitment),
            Err(e) => Err(e.clone()),
        }
    }
}

impl ProposalManager {
    pub fn new(
        block_builder_factory: Arc<dyn BlockBuilderFactoryTrait + Send + Sync>,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    ) -> Self {
        Self {
            storage_reader,
            active_proposal: Arc::new(Mutex::new(None)),
            block_builder_factory,
            active_proposal_handle: None,
            active_height: None,
            executed_proposals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn spawn_build_block_task(
        &mut self,
        proposal_id: ProposalId,
        mut block_builder: Box<dyn BlockBuilderTrait>,
    ) {
        let active_proposal = self.active_proposal.clone();
        let executed_proposals = self.executed_proposals.clone();

        self.active_proposal_handle = Some(tokio::spawn(
            async move {
                let result = block_builder
                    .build_block()
                    .await
                    .map(ProposalOutput::from)
                    .map_err(|e| GetProposalResultError::BlockBuilderError(Arc::new(e)));

                executed_proposals.lock().await.insert(proposal_id, result);

                // The proposal is done, clear the active proposal.
                let mut active_proposal = active_proposal.lock().await;
                if let Some(current_active_proposal_id) = *active_proposal {
                    if current_active_proposal_id == proposal_id {
                        active_proposal.take();
                    }
                }
            }
            .in_current_span(),
        ));
    }

    async fn reset_active_height(&mut self) {
        if let Some(_active_proposal) = self.active_proposal.lock().await.take() {
            // TODO: Abort the block_builder.
        }
        self.executed_proposals.lock().await.clear();
        self.active_height = None;
    }

    // Sets a new active proposal task.
    // Fails if either there is no active height, there is another proposal being generated, or a
    // proposal with the same ID already exists.
    async fn set_active_proposal(
        &mut self,
        proposal_id: ProposalId,
    ) -> Result<(), BuildProposalError> {
        self.active_height.ok_or(BuildProposalError::NoActiveHeight)?;

        if self.executed_proposals.lock().await.contains_key(&proposal_id) {
            return Err(BuildProposalError::ProposalAlreadyExists { proposal_id });
        }

        let mut active_proposal = self.active_proposal.lock().await;
        if let Some(current_generating_proposal_id) = *active_proposal {
            return Err(BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id: proposal_id,
            });
        }

        debug!("Set proposal {} as the one being generated.", proposal_id);
        *active_proposal = Some(proposal_id);
        Ok(())
    }

    // This function assumes there are not requests processed in parallel by the batcher, otherwise
    // there is a race conditon between creating the active_proposal_handle and awaiting on it.
    pub async fn await_proposal_completion(&mut self, proposal_id: ProposalId) {
        if self.active_proposal.lock().await.as_ref() == Some(&proposal_id) {
            let _ = self
                .active_proposal_handle
                .take()
                .expect("Active proposal handle should exist.")
                .await;
        }
    }

    // A helper function for testing purposes (to be able to await the active proposal).
    // Returns true if there was an active porposal, and false otherwise.
    // TODO: Consider making the tests a nested module to allow them to access private members.
    // TODO(yael 5/1/2024): use wait_for_proposal_completion instead of this function.
    #[cfg(test)]
    pub async fn await_active_proposal(&mut self) -> bool {
        if let Some(handle) = self.active_proposal_handle.take() {
            handle.await.unwrap();
            return true;
        }
        false
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
