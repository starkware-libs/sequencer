use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::abi::constants;
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
    BlockBuilderExecutionParams,
    BlockBuilderFactoryTrait,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    BlockMetadata,
};
use crate::transaction_provider::{ProposeTransactionProvider, ValidateTransactionProvider};

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
    #[error("Already working on height.")]
    HeightInProgress,
}

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
    #[error("Retrospective block hash is missing.")]
    MissingRetrospectiveBlockHash,
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
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError>;

    async fn propose_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), GenerateProposalError>;

    async fn validate_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: ValidateTransactionProvider,
    ) -> Result<(), GenerateProposalError>;

    async fn take_proposal_result(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalOutput>;

    async fn get_proposal_status(&self, proposal_id: ProposalId) -> InternalProposalStatus;

    async fn await_proposal_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalResult<ProposalCommitment>;

    async fn abort_proposal(&mut self, proposal_id: ProposalId);
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
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
pub(crate) struct ProposalManager {
    storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    active_height: Option<BlockNumber>,

    /// The block proposal that is currently being built, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_task: Option<ProposalTask>,

    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait + Send + Sync>,
    executed_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult<ProposalOutput>>>>,
}

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
        if self.active_height == Some(height) {
            return Err(StartHeightError::HeightInProgress);
        }

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
        self.reset_active_height(height).await;
        Ok(())
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    /// Requires tx_sender for sending the generated transactions to the caller.
    #[instrument(skip(self, tx_sender, tx_provider), err, fields(self.active_height))]
    async fn propose_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
        tx_provider: ProposeTransactionProvider,
    ) -> Result<(), GenerateProposalError> {
        self.set_active_proposal(proposal_id, retrospective_block_hash).await?;

        info!("Starting generation of a new proposal with id {}.", proposal_id);

        // Create the block builder, and a channel to allow aborting the block building task.
        let (abort_signal_sender, abort_signal_receiver) = tokio::sync::oneshot::channel();
        let height = self.active_height.expect("No active height.");

        let block_builder = self.block_builder_factory.create_block_builder(
            BlockMetadata { height, retrospective_block_hash },
            BlockBuilderExecutionParams { deadline, fail_on_err: false },
            Box::new(tx_provider),
            Some(tx_sender.clone()),
            abort_signal_receiver,
        )?;

        let join_handle = self.spawn_build_block_task(proposal_id, block_builder).await;
        self.active_proposal_task = Some(ProposalTask { abort_signal_sender, join_handle });

        Ok(())
    }

    /// Starts validation of a block proposal for the given proposal_id and height with
    /// transactions from tx_receiver channel.
    #[instrument(skip(self, tx_provider), err, fields(self.active_height))]
    async fn validate_block(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: ValidateTransactionProvider,
    ) -> Result<(), GenerateProposalError> {
        self.set_active_proposal(proposal_id, retrospective_block_hash).await?;

        info!("Starting validation of proposal with id {}.", proposal_id);

        // Create the block builder, and a channel to allow aborting the block building task.
        let (abort_signal_sender, abort_signal_receiver) = tokio::sync::oneshot::channel();
        let height = self.active_height.expect("No active height.");

        let block_builder = self.block_builder_factory.create_block_builder(
            BlockMetadata { height, retrospective_block_hash },
            BlockBuilderExecutionParams { deadline, fail_on_err: true },
            Box::new(tx_provider),
            None,
            abort_signal_receiver,
        )?;

        let join_handle = self.spawn_build_block_task(proposal_id, block_builder).await;
        self.active_proposal_task = Some(ProposalTask { abort_signal_sender, join_handle });

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
    ) -> ProposalResult<ProposalCommitment> {
        if *self.active_proposal.lock().await == Some(proposal_id) {
            self.await_active_proposal().await;
        }
        let proposals = self.executed_proposals.lock().await;
        let output = proposals
            .get(&proposal_id)
            .ok_or(GetProposalResultError::ProposalDoesNotExist { proposal_id })?;
        match output {
            Ok(output) => Ok(output.commitment),
            Err(e) => Err(e.clone()),
        }
    }

    // Aborts the proposal with the given ID, if active.
    // Should be used in validate flow, if the consensus decides to abort the proposal.
    async fn abort_proposal(&mut self, proposal_id: ProposalId) {
        if *self.active_proposal.lock().await == Some(proposal_id) {
            self.abort_active_proposal().await;
            self.executed_proposals
                .lock()
                .await
                .insert(proposal_id, Err(GetProposalResultError::Aborted));
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
            active_proposal_task: None,
            active_height: None,
            executed_proposals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn spawn_build_block_task(
        &mut self,
        proposal_id: ProposalId,
        mut block_builder: Box<dyn BlockBuilderTrait>,
    ) -> tokio::task::JoinHandle<()> {
        let active_proposal = self.active_proposal.clone();
        let executed_proposals = self.executed_proposals.clone();

        tokio::spawn(
            async move {
                let result = block_builder
                    .build_block()
                    .await
                    .map(ProposalOutput::from)
                    .map_err(|e| GetProposalResultError::BlockBuilderError(Arc::new(e)));

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
        )
    }

    async fn reset_active_height(&mut self, new_height: BlockNumber) {
        self.abort_active_proposal().await;
        self.executed_proposals.lock().await.clear();
        self.active_height = Some(new_height);
    }

    // Sets a new active proposal task.
    // Fails if either there is no active height, there is another proposal being generated, or a
    // proposal with the same ID already exists.
    async fn set_active_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockHashAndNumber>,
    ) -> Result<(), GenerateProposalError> {
        let height = self.active_height.ok_or(GenerateProposalError::NoActiveHeight)?;

        if height >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)
            && retrospective_block_hash.is_none()
        {
            return Err(GenerateProposalError::MissingRetrospectiveBlockHash);
        }

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

    // Awaits the active proposal.
    // Returns true if there was an active proposal, and false otherwise.
    pub async fn await_active_proposal(&mut self) -> bool {
        if let Some(proposal_task) = self.active_proposal_task.take() {
            proposal_task.join_handle.await.ok();
            return true;
        }
        false
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
