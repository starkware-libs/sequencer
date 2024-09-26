use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::select;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument, trace, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalManagerConfig {
    pub block_builder_next_txs_buffer_size: usize,
    pub max_txs_per_mempool_request: usize,
    pub outstream_content_buffer_size: usize,
}

impl Default for ProposalManagerConfig {
    fn default() -> Self {
        // TODO: Get correct default values.
        Self {
            block_builder_next_txs_buffer_size: 100,
            max_txs_per_mempool_request: 10,
            outstream_content_buffer_size: 100,
        }
    }
}

impl SerializeConfig for ProposalManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "block_builder_next_txs_buffer_size",
                &self.block_builder_next_txs_buffer_size,
                "Maximum transactions to fill in the stream buffer for the block builder before \
                 blocking",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool per iteration of proposal generation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "outstream_content_buffer_size",
                &self.outstream_content_buffer_size,
                "Maximum items to add to the outstream buffer before blocking",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Error)]
pub enum ProposalManagerError {
    #[error(
        "Received proposal generation request with id {new_proposal_id} while already generating \
         proposal with id {current_generating_proposal_id}."
    )]
    AlreadyGeneratingProposal {
        current_generating_proposal_id: ProposalId,
        new_proposal_id: ProposalId,
    },
    #[error("Internal error.")]
    InternalError,
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

pub type ProposalsManagerResult<T> = Result<T, ProposalManagerError>;

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
// TODO: Remove dead_code attribute.
#[allow(dead_code)]
pub(crate) struct ProposalManager {
    config: ProposalManagerConfig,
    mempool_client: SharedMempoolClient,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_handle: Option<ActiveTaskHandle>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait>,
}

type ActiveTaskHandle = tokio::task::JoinHandle<ProposalsManagerResult<()>>;

impl ProposalManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(
        config: ProposalManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactoryTrait>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            active_proposal: Arc::new(Mutex::new(None)),
            block_builder_factory,
            active_proposal_handle: None,
        }
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    /// Requires output_content_sender for sending the generated transactions to the caller.
    #[instrument(skip(self, output_content_sender), err)]
    pub async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
        // TODO: Should this be an unbounded channel?
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> ProposalsManagerResult<()> {
        info!("Starting generation of a new proposal with id {}.", proposal_id);
        self.set_active_proposal(proposal_id).await?;

        self.active_proposal_handle = Some(tokio::spawn(
            BuildProposalTask {
                mempool_client: self.mempool_client.clone(),
                output_content_sender,
                block_builder_next_txs_buffer_size: self.config.block_builder_next_txs_buffer_size,
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                block_builder_factory: self.block_builder_factory.clone(),
                active_proposal: self.active_proposal.clone(),
                deadline,
            }
            .run()
            .in_current_span(),
        ));

        Ok(())
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_active_proposal(&mut self, proposal_id: ProposalId) -> ProposalsManagerResult<()> {
        let mut lock = self.active_proposal.lock().await;

        if let Some(active_proposal) = *lock {
            return Err(ProposalManagerError::AlreadyGeneratingProposal {
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
    pub async fn await_active_proposal(&mut self) -> Option<ProposalsManagerResult<()>> {
        match self.active_proposal_handle.take() {
            Some(handle) => Some(handle.await.unwrap()),
            None => None,
        }
    }
}

#[allow(dead_code)]
struct BuildProposalTask {
    mempool_client: SharedMempoolClient,
    output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    max_txs_per_mempool_request: usize,
    block_builder_next_txs_buffer_size: usize,
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait>,
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    deadline: tokio::time::Instant,
}

#[allow(dead_code)]
impl BuildProposalTask {
    async fn run(mut self) -> ProposalsManagerResult<()> {
        // We convert the receiver to a stream and pass it to the block builder while using the
        // sender to feed the stream.
        let block_builder = self.block_builder_factory.create_block_builder();
        let (mempool_tx_sender, mempool_tx_receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.block_builder_next_txs_buffer_size);
        let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);
        let building_future = block_builder.build_block(
            self.deadline,
            mempool_tx_stream,
            self.output_content_sender.clone(),
        );

        let feed_mempool_txs_future = Self::feed_mempool_txs(
            &self.mempool_client,
            self.max_txs_per_mempool_request,
            &mempool_tx_sender,
        );

        // Wait for either the block builder to finish or the feeding of transactions to error.
        // The other task will be cancelled.
        let res = select! {
            // This will send txs from the mempool to the stream we provided to the block builder.
            feeding_error = feed_mempool_txs_future => {
                error!("Failed to feed more mempool txs: {}.", feeding_error);
                // TODO: Notify the mempool about remaining txs.
                // TODO: Abort the block builder or wait for it to finish.
                Err(feeding_error)
            },
            builder_done = building_future => {
                info!("Block builder finished.");
                Ok(builder_done)
            }
        };
        self.active_proposal_finished().await;
        res
    }

    /// Feeds transactions from the mempool to the mempool_tx_sender channel.
    /// Returns only on error or when the task is cancelled.
    async fn feed_mempool_txs(
        mempool_client: &SharedMempoolClient,
        max_txs_per_mempool_request: usize,
        mempool_tx_sender: &tokio::sync::mpsc::Sender<Transaction>,
    ) -> ProposalManagerError {
        loop {
            // TODO: Get L1 transactions.
            let mempool_txs = match mempool_client.get_txs(max_txs_per_mempool_request).await {
                Ok(txs) if txs.is_empty() => {
                    // TODO: Consider sleeping for a while.
                    tokio::task::yield_now().await;
                    continue;
                }
                Ok(txs) => txs,
                Err(e) => return e.into(),
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
pub type OutputTxStream = ReceiverStream<Transaction>;

#[async_trait]
pub trait BlockBuilderTrait: Send + Sync {
    async fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    );
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderFactoryTrait: Send + Sync {
    fn create_block_builder(&self) -> Arc<dyn BlockBuilderTrait>;
}
