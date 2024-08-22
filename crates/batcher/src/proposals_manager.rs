use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalsManagerConfig {
    pub max_txs_per_mempool_request: usize,
    pub outstream_content_buffer_size: usize,
}

impl Default for ProposalsManagerConfig {
    fn default() -> Self {
        // TODO: Get correct value for default max_txs_per_mempool_request.
        Self { max_txs_per_mempool_request: 10, outstream_content_buffer_size: 100 }
    }
}

impl SerializeConfig for ProposalsManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool per iteration of proposal \
                 generation.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "outstream_content_buffer_size",
                &self.outstream_content_buffer_size,
                "Maximum items to add to the outstream buffer before blocking further filling of \
                 the stream.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Debug, Error)]
pub enum ProposalsManagerError {
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

pub type ProposalsManagerResult<T> = Result<T, ProposalsManagerError>;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ProposalsManagerTrait: Send + Sync {
    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    async fn generate_block_proposal(
        &mut self,
        timeout: tokio::time::Instant,
        _height: BlockNumber,
    ) -> ProposalsManagerResult<(
        JoinHandle<ProposalsManagerResult<()>>,
        BoxStream<'static, Transaction>,
    )>;
}

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
pub(crate) struct ProposalsManager {
    config: ProposalsManagerConfig,
    mempool_client: SharedMempoolClient,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactory>,
    proposal_id_marker: ProposalId,
}

impl ProposalsManager {
    pub fn new(
        config: ProposalsManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactory>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            proposal_in_generation: Arc::new(Mutex::new(None)),
            block_builder_factory,
            proposal_id_marker: 0,
        }
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_proposal_in_generation(
        &mut self,
        proposal_id: ProposalId,
    ) -> ProposalsManagerResult<()> {
        let mut lock = self.proposal_in_generation.lock().await;

        if let Some(proposal_in_generation) = *lock {
            return Err(ProposalsManagerError::AlreadyGeneratingProposal {
                current_generating_proposal_id: proposal_in_generation,
                new_proposal_id: proposal_id,
            });
        }

        *lock = Some(proposal_id);
        debug!("Set proposal {} as the one being generated.", proposal_id);
        Ok(())
    }

    #[instrument(skip(mempool_client, mempool_tx_sender))]
    async fn feed_txs_to_block_builder(
        timeout: tokio::time::Instant,
        mempool_client: SharedMempoolClient,
        max_txs_per_mempool_request: usize,
        mempool_tx_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> ProposalsManagerResult<()> {
        loop {
            if tokio::time::Instant::now() > timeout {
                info!("Proposal reached timeout.");
                return Ok(());
            }
            let mempool_txs = mempool_client.get_txs(max_txs_per_mempool_request).await?;
            if mempool_txs.is_empty() {
                // TODO: check if sleep is needed here.
                tokio::task::yield_now().await;
                continue;
            }
            for tx in mempool_txs {
                mempool_tx_sender.send(tx).await.map_err(|err| {
                    // TODO: should we return the rest of the txs to the mempool?
                    error!("Failed to send transaction to the block builder: {}.", err);
                    ProposalsManagerError::InternalError
                })?;
            }
        }
    }
}

#[async_trait]
impl ProposalsManagerTrait for ProposalsManager {
    #[instrument(skip(self), fields(proposal_id))]
    async fn generate_block_proposal(
        &mut self,
        timeout: tokio::time::Instant,
        _height: BlockNumber,
    ) -> ProposalsManagerResult<(
        JoinHandle<ProposalsManagerResult<()>>,
        BoxStream<'static, Transaction>,
    )> {
        let proposal_id = self.proposal_id_marker;
        self.proposal_id_marker += 1;
        info!("Starting generation of a new proposal with id {}.", proposal_id);
        self.set_proposal_in_generation(proposal_id).await?;

        // TODO: Should we use a different config for the stream buffer size?
        // We convert the receiver to a stream and pass it to the block builder while using the
        // sender to feed the stream.
        let (mempool_tx_sender, mempool_tx_receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.config.max_txs_per_mempool_request);
        let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);

        let block_builder = self.block_builder_factory.create_block_builder();

        let (executed_txs_stream, builder_finished_receiver) =
            block_builder.start(mempool_tx_stream);

        // Clones that will move to a new task.
        let mempool_client = self.mempool_client.clone();
        let max_txs_per_mempool_request = self.config.max_txs_per_mempool_request;
        let proposal_in_generation = self.proposal_in_generation.clone();

        // Feed transactions to the block builder until it sends a signal that it finished.
        let handle = tokio::spawn(
            async move {
                let res = tokio::select! {
                    // This will send txs from the mempool to the stream we provided to the block builder.
                    feeder_stopped = Self::feed_txs_to_block_builder(
                        timeout,
                        mempool_client,
                        max_txs_per_mempool_request,
                        mempool_tx_sender
                    ) => feeder_stopped,
                    finished_block = builder_finished_receiver => {
                        if let Err(err) = finished_block {
                            error!("Failed to receive block builder finished signal: {}.", err);
                        }
                        info!("Closing block.");
                        // TODO: Get state diff.
                        Ok(())
                    }
                };
                let mut proposal_id = proposal_in_generation.lock().await;
                *proposal_id = None;
                res
            }
            .in_current_span(),
        );

        Ok((handle, executed_txs_stream.boxed()))
    }
}

pub type InputTxStream = ReceiverStream<Transaction>;
pub type OutputTxStream = ReceiverStream<Transaction>;

#[async_trait]
#[cfg_attr(test, automock)]
pub trait BlockBuilderTrait: Send + Sync {
    fn start(
        &self,
        input_txs_stream: InputTxStream,
    ) -> (OutputTxStream, tokio::sync::oneshot::Receiver<bool>);
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderFactory: Send + Sync {
    fn create_block_builder(&self) -> Arc<dyn BlockBuilderTrait>;
}

pub(crate) struct BlockBuilderFactoryImpl {}

impl BlockBuilderFactory for BlockBuilderFactoryImpl {
    fn create_block_builder(&self) -> Arc<dyn BlockBuilderTrait> {
        // TODO: Implement.
        unimplemented!()
    }
}
