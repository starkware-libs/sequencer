use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::blockifier::block::BlockNumberHashPair;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::select;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument, trace, Instrument};

use crate::batcher::BatcherStorageReaderTrait;
use crate::block_builder::{BlockBuilderError, BlockBuilderFactoryTrait, BlockBuilderTrait};

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
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error("No active height to work on.")]
    NoActiveHeight,
}

#[async_trait]
pub trait ProposalManagerTrait: Send + Sync {
    fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError>;

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        retrospective_block_hash: Option<BlockNumberHashPair>,
        deadline: tokio::time::Instant,
        tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    ) -> Result<(), BuildProposalError>;
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
}

type ActiveTaskHandle = tokio::task::JoinHandle<Result<(), BuildProposalError>>;

#[async_trait]
impl ProposalManagerTrait for ProposalManager {
    /// Starts working on the given height.
    #[instrument(skip(self), err)]

    fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError> {
        // TODO: handle the case when `next_height==height && active_height<next_height` - can
        // happen if the batcher got out of sync and got re-synced manually.
        if let Some(active_height) = self.active_height {
            return Err(StartHeightError::AlreadyWorkingOnHeight {
                active_height,
                new_height: height,
            });
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
        if self.active_height.is_none() {
            return Err(BuildProposalError::NoActiveHeight);
        }
        info!("Starting generation of a new proposal with id {}.", proposal_id);
        self.set_active_proposal(proposal_id).await?;

        // TODO(yael 7/10/2024) : pass the real block_number instead of 0
        let block_builder = self
            .block_builder_factory
            .create_block_builder(BlockNumber(0), retrospective_block_hash)?;

        self.active_proposal_handle = Some(tokio::spawn(
            BuildProposalTask {
                mempool_client: self.mempool_client.clone(),
                tx_sender,
                block_builder_next_txs_buffer_size: self.config.block_builder_next_txs_buffer_size,
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                block_builder,
                active_proposal: self.active_proposal.clone(),
                deadline,
            }
            .run()
            .in_current_span(),
        ));

        Ok(())
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
        }
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_active_proposal(
        &mut self,
        proposal_id: ProposalId,
    ) -> Result<(), BuildProposalError> {
        let mut lock = self.active_proposal.lock().await;

        if let Some(active_proposal) = *lock {
            return Err(BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id: active_proposal,
                new_proposal_id: proposal_id,
            });
        }

        *lock = Some(proposal_id);
        debug!("Set proposal {} as the one being generated.", proposal_id);
        Ok(())
    }

    // A helper function for testing purposes (to be able to await the active proposal).
    // TODO: Consider making the tests a nested module to allow them to access private members.
    #[cfg(test)]
    pub async fn await_active_proposal(&mut self) {
        match self.active_proposal_handle.take() {
            Some(handle) => Some(handle.await.unwrap()),
            None => None,
        };
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
}

impl BuildProposalTask {
    async fn run(mut self) -> Result<(), BuildProposalError> {
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

        // Wait for either the block builder to finish or the feeding of transactions to error.
        // The other task will be cancelled.
        let _res = select! {
            // This will send txs from the mempool to the stream we provided to the block builder.
            feeding_error = feed_mempool_txs_future => {
                error!("Failed to feed more mempool txs: {}.", feeding_error);
                // TODO: Notify the mempool about remaining txs.
                // TODO: Abort the block builder or wait for it to finish.
                Err(feeding_error)
            },
            builder_done = building_future => {
                info!("Block builder finished.");
                // TODO: Save the output in self.proposals.
                Ok(builder_done)
            }
        };
        self.active_proposal_finished().await;
        // TODO: store the block artifacts, return the state_diff
        Ok(())
    }

    /// Feeds transactions from the mempool to the mempool_tx_sender channel.
    /// Returns only on error or when the task is cancelled.
    async fn feed_mempool_txs(
        mempool_client: &SharedMempoolClient,
        max_txs_per_mempool_request: usize,
        mempool_tx_sender: &tokio::sync::mpsc::Sender<Transaction>,
    ) -> BuildProposalError {
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

    async fn active_proposal_finished(&mut self) {
        let mut proposal_id = self.active_proposal.lock().await;
        *proposal_id = None;
    }
}

pub type InputTxStream = ReceiverStream<Transaction>;
