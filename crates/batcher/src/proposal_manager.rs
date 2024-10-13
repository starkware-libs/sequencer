use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::blockifier::block::BlockNumberHashPair;
use indexmap::IndexMap;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{ProposalCommitment, ProposalId};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::select;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument, trace, Instrument};

use crate::batcher::BatcherStorageReaderTrait;
use crate::block_builder::{
    BlockBuilderError,
    BlockBuilderFactoryTrait,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalManagerConfig {
    pub block_builder_next_txs_buffer_size: usize,
    pub max_txs_per_mempool_request: usize,
}

impl Default for ProposalManagerConfig {
    fn default() -> Self {
        // TODO: Get correct default values.
        Self { block_builder_next_txs_buffer_size: 100, max_txs_per_mempool_request: 10 }
    }
}

impl SerializeConfig for ProposalManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "block_builder_next_txs_buffer_size",
                &self.block_builder_next_txs_buffer_size,
                "Maximum transactions to fill in the stream buffer for the block builder before \
                 blocking further filling of the stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool in a single get_txs request.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Debug, Error)]
pub enum StartHeightError {
    #[error("Can't start new height {new_height} while working on height {active_height}.")]
    AlreadyWorkingOnHeight { active_height: BlockNumber, new_height: BlockNumber },
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

#[derive(Debug, Error)]
pub enum GetProposalResultError {
    #[error(transparent)]
    BlockBuilderError(#[from] BlockBuilderError),
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error("Proposal with id {proposal_id} does not exist.")]
    ProposalDoesNotExist { proposal_id: ProposalId },
}

#[async_trait]
pub trait ProposalManagerTrait: Send + Sync {
    async fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError>;

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockNumberHashPair>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> Result<(), BuildProposalError>;

    async fn get_proposal_result(&mut self, proposal_id: ProposalId) -> ProposalResult;

    async fn get_done_proposal_commitment(
        &self,
        proposal_id: ProposalId,
    ) -> Option<ProposalCommitment>;
}

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
pub(crate) struct ProposalManager {
    config: ProposalManagerConfig,
    mempool_client: SharedMempoolClient,
    storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    active_height: Option<BlockNumber>,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_handle: Option<ActiveTaskHandle>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait + Send + Sync>,
    done_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult>>>,
}

type ActiveTaskHandle = tokio::task::JoinHandle<()>;
pub type ProposalResult = Result<ProposalOutput, GetProposalResultError>;

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
    #[instrument(skip(self, tx_sender), err, fields(self.active_height))]
    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockNumberHashPair>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> Result<(), BuildProposalError> {
        let height = self.active_height.ok_or(BuildProposalError::NoActiveHeight)?;
        if self.done_proposals.lock().await.contains_key(&proposal_id) {
            return Err(BuildProposalError::ProposalAlreadyExists { proposal_id });
        }
        info!("Starting generation of a new proposal with id {}.", proposal_id);
        self.set_active_proposal(proposal_id).await?;
        let block_builder =
            self.block_builder_factory.create_block_builder(height, retrospective_block_hash)?;

        self.active_proposal_handle = Some(tokio::spawn(
            BuildProposalTask {
                mempool_client: self.mempool_client.clone(),
                tx_sender,
                block_builder_next_txs_buffer_size: self.config.block_builder_next_txs_buffer_size,
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                block_builder,
                active_proposal: self.active_proposal.clone(),
                deadline,
                done_proposals: self.done_proposals.clone(),
            }
            .run()
            .in_current_span(),
        ));

        Ok(())
    }

    async fn get_proposal_result(&mut self, proposal_id: ProposalId) -> ProposalResult {
        self.done_proposals
            .lock()
            .await
            .remove(&proposal_id)
            .ok_or(GetProposalResultError::ProposalDoesNotExist { proposal_id })?
    }

    async fn get_done_proposal_commitment(
        &self,
        proposal_id: ProposalId,
    ) -> Option<ProposalCommitment> {
        Some(self.done_proposals.lock().await.get(&proposal_id)?.as_ref().ok()?.commitment)
    }
}

impl ProposalManager {
    pub fn new(
        config: ProposalManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactoryTrait + Send + Sync>,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            storage_reader,
            active_proposal: Arc::new(Mutex::new(None)),
            block_builder_factory,
            active_proposal_handle: None,
            active_height: None,
            done_proposals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn reset_active_height(&mut self) {
        if let Some(_active_proposal) = self.active_proposal.lock().await.take() {
            // TODO: Abort the block_builder.
        }
        self.done_proposals.lock().await.clear();
        self.active_height = None;
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_active_proposal(
        &mut self,
        active_proposal: ProposalId,
    ) -> Result<(), BuildProposalError> {
        let mut current_active_proposal = self.active_proposal.lock().await;
        if let Some(current_generating_proposal_id) = *current_active_proposal {
            return Err(BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id: active_proposal,
            });
        }

        *current_active_proposal = Some(active_proposal);
        debug!("Set proposal {} as the one being generated.", active_proposal);
        Ok(())
    }

    // A helper function for testing purposes (to be able to await the active proposal).
    // TODO: Consider making the tests a nested module to allow them to access private members.
    #[cfg(test)]
    pub async fn await_active_proposal(&mut self) {
        if let Some(handle) = self.active_proposal_handle.take() {
            handle.await.unwrap();
        }
    }
}

struct BuildProposalTask {
    mempool_client: SharedMempoolClient,
    tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    max_txs_per_mempool_request: usize,
    block_builder_next_txs_buffer_size: usize,
    block_builder: Box<dyn BlockBuilderTrait + Send>,
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    deadline: tokio::time::Instant,
    done_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult>>>,
}

impl BuildProposalTask {
    async fn run(mut self) {
        // We convert the receiver to a stream and pass it to the block builder while using the
        // sender to feed the stream.
        let (mempool_tx_sender, mempool_tx_receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.block_builder_next_txs_buffer_size);
        let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);
        let building_future = self.block_builder.build_block(
            self.deadline,
            mempool_tx_stream,
            self.tx_sender.clone(),
        );

        let feed_mempool_txs_future = Self::feed_mempool_txs(
            &self.mempool_client,
            self.max_txs_per_mempool_request,
            &mempool_tx_sender,
        );

        // Wait for one of the following:
        // * block builder finished
        // * the feeding of transactions errored
        // The other tasks will be cancelled.
        let result = select! {
            // This will send txs from the mempool to the stream we provided to the block builder.
            feeding_error = feed_mempool_txs_future => {
                error!("Failed to feed more mempool txs: {}.", feeding_error);
                // TODO: Notify the mempool about remaining txs.
                // TODO: Abort the block builder.
                Err(feeding_error)
            },
            builder_done = building_future => {
                info!("Block builder finished.");
                builder_done.map(ProposalOutput::from).map_err(GetProposalResultError::BlockBuilderError)
            }
        };
        self.mark_active_proposal_as_done(result).await;
    }

    /// Feeds transactions from the mempool to the mempool_tx_sender channel.
    /// Returns only on error or when the task is cancelled.
    async fn feed_mempool_txs(
        mempool_client: &SharedMempoolClient,
        max_txs_per_mempool_request: usize,
        mempool_tx_sender: &tokio::sync::mpsc::Sender<Transaction>,
    ) -> GetProposalResultError {
        loop {
            // TODO: Get L1 transactions.
            let mempool_txs = match mempool_client.get_txs(max_txs_per_mempool_request).await {
                Ok(txs) if txs.is_empty() => {
                    // TODO: Consider sleeping for a while.
                    tokio::task::yield_now().await;
                    continue;
                }
                Ok(txs) => txs,
                Err(e) => {
                    error!("MempoolError: {}", e);
                    return e.into();
                }
            };
            trace!(
                "Feeding {} transactions from the mempool to the block builder.",
                mempool_txs.len()
            );
            for tx in mempool_txs {
                mempool_tx_sender
                    .send(tx)
                    .await
                    .expect("Channel should remain open during feeding mempool transactions.");
            }
        }
    }

    async fn mark_active_proposal_as_done(self, result: ProposalResult) {
        let proposal_id =
            self.active_proposal.lock().await.take().expect("Active proposal should exist.");
        self.done_proposals.lock().await.insert(proposal_id, result);
    }
}

pub type InputTxStream = ReceiverStream<Transaction>;

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
