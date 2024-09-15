use std::collections::BTreeMap;
use std::sync::Arc;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::{pin, select};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument, trace, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalManagerConfig {
    pub max_txs_per_mempool_request: usize,
    pub outstream_content_buffer_size: usize,
}

impl Default for ProposalManagerConfig {
    fn default() -> Self {
        // TODO: Get correct value for default max_txs_per_mempool_request.
        Self { max_txs_per_mempool_request: 10, outstream_content_buffer_size: 100 }
    }
}

impl SerializeConfig for ProposalManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
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
}

impl ProposalManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(config: ProposalManagerConfig, mempool_client: SharedMempoolClient) -> Self {
        Self { config, mempool_client, active_proposal: Arc::new(Mutex::new(None)) }
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

        let block_builder = block_builder::BlockBuilder {};
        let _handle = tokio::spawn(
            BuildProposalTask {
                mempool_client: self.mempool_client.clone(),
                output_content_sender,
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                block_builder,
                active_proposal: self.active_proposal.clone(),
                deadline,
            }
            .run()
            .in_current_span(),
        );

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
}

// TODO: Should be defined elsewhere.
#[allow(dead_code)]
mod block_builder {
    use starknet_api::executable_transaction::Transaction;
    use tokio_stream::Stream;

    #[derive(Debug, PartialEq)]
    pub enum Status {
        Building,
        Ready,
        Timeout,
    }

    pub struct BlockBuilder {}

    impl BlockBuilder {
        pub async fn build_block(
            &self,
            _deadline: tokio::time::Instant,
            _mempool_tx_stream: impl Stream<Item = Transaction>,
            _output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
        ) {
        }
    }
}

#[allow(dead_code)]
struct BuildProposalTask {
    mempool_client: SharedMempoolClient,
    output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    max_txs_per_mempool_request: usize,
    block_builder: block_builder::BlockBuilder,
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    deadline: tokio::time::Instant,
}

#[allow(dead_code)]
impl BuildProposalTask {
    async fn run(mut self) -> ProposalsManagerResult<()> {
        let res = {
            // TODO: Should we use a different config for the stream buffer size?
            // We convert the receiver to a stream and pass it to the block builder while using the
            // sender to feed the stream.
            let (mempool_tx_sender, mempool_tx_receiver) =
                tokio::sync::mpsc::channel::<Transaction>(self.max_txs_per_mempool_request);
            let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);
            // Need to pin the future to be able to use it in multiple select! expressions.
            // See: https://docs.rs/tokio/latest/tokio/macro.select.html#:~:text=Using%20the%20same%20future%20in%20multiple%20select!%20expressions%20can%20be%20done%20by%20passing%20a%20reference%20to%20the%20future.%20Doing%20so%20requires%20the%20future%20to%20be%20Unpin.%20A%20future%20can%20be%20made%20Unpin%20by%20either%20using%20Box%3A%3Apin%20or%20stack%20pinning.
            let building_future = self.block_builder.build_block(
                self.deadline,
                mempool_tx_stream,
                self.output_content_sender.clone(),
            );
            pin!(building_future);
            loop {
                select! {
                    // This will send txs from the mempool to the stream we provided to the block builder.
                    fetch_result = Self::feed_more_mempool_txs(
                        &self.mempool_client,
                        self.max_txs_per_mempool_request,
                        &mempool_tx_sender,
                    ) => {
                        if let Err(err) = fetch_result {
                            error!("Failed to feed more mempool txs: {}.", err);
                            // TODO: Notify the mempool about remaining txs.
                            break Err(err);
                        }
                        continue;
                    },
                    builder_done = &mut building_future => {
                        info!("Block builder finished.");
                        break Ok(builder_done);
                    }
                };
            }
        };
        self.active_proposal_finished().await;
        res
    }

    async fn feed_more_mempool_txs(
        mempool_client: &SharedMempoolClient,
        max_txs_per_mempool_request: usize,
        mempool_tx_sender: &tokio::sync::mpsc::Sender<Transaction>,
    ) -> ProposalsManagerResult<()> {
        // TODO: Get L1 transactions.
        let mempool_txs = mempool_client.get_txs(max_txs_per_mempool_request).await?;
        trace!("Feeding {} transactions from the mempool to the block builder.", mempool_txs.len());
        for tx in mempool_txs {
            mempool_tx_sender.send(tx).await.map_err(|err| {
                // TODO: should we return the rest of the txs to the mempool?
                error!("Failed to send transaction to the block builder: {}.", err);
                ProposalManagerError::InternalError
            })?;
        }
        Ok(())
    }

    async fn active_proposal_finished(&mut self) {
        let mut proposal_id = self.active_proposal.lock().await;
        *proposal_id = None;
    }
}
