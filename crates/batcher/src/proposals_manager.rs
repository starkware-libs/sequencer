use std::collections::BTreeMap;
use std::sync::Arc;

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
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tracing::{debug, error, info, instrument, Instrument};

// TODO: Should be defined in SN_API probably (shared with the consensus).
pub type ProposalId = u64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalsManagerConfig {
    pub max_txs_per_mempool_request: usize,
    pub outstream_tx_stream_buffer_size: usize,
}

impl Default for ProposalsManagerConfig {
    fn default() -> Self {
        // TODO: Get correct value for default max_txs_per_mempool_request.
        Self { max_txs_per_mempool_request: 10, outstream_tx_stream_buffer_size: 100 }
    }
}

impl SerializeConfig for ProposalsManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool per iteration of proposal generation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "outstream_tx_stream_buffer_size",
                &self.outstream_tx_stream_buffer_size,
                "Maximum transactions to add to the outstream buffer before blocking",
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

/// Main struct for handling block proposals.
/// Taking care of:
/// - Proposing new blocks.
/// - Validating incoming proposals.
/// - Commiting accepted proposals to the storage.
///
/// Triggered by the consensus.
// TODO: Remove dead_code attribute.
#[allow(dead_code)]
pub(crate) struct ProposalsManager {
    config: ProposalsManagerConfig,
    mempool_client: SharedMempoolClient,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    // To be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactory>,
}

impl ProposalsManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
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
        }
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    #[instrument(skip(self))]
    pub async fn generate_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        timeout: tokio::time::Instant,
        _height: BlockNumber,
    ) -> ProposalsManagerResult<impl Stream<Item = Transaction>> {
        info!("Starting generation of new proposal.");
        self.set_proposal_in_generation(proposal_id).await?;

        let (sender, receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.config.outstream_tx_stream_buffer_size);
        // TODO: Find where to join the task - needed to make sure it starts immediatly.
        let _handle = tokio::spawn(
            ProposalGenerationTask {
                timeout,
                mempool_client: self.mempool_client.clone(),
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                sender,
                proposal_in_generation: self.proposal_in_generation.clone(),
                block_builder_factory: self.block_builder_factory.clone(),
            }
            .run()
            .in_current_span(),
        );

        Ok(ReceiverStream::new(receiver))
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
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderTrait: Send {
    /// Returning true if the block is ready to be proposed.
    fn add_txs(&self, txs: &[Transaction]) -> bool;
}

#[allow(dead_code)]
#[cfg_attr(test, automock)]
pub(crate) trait BlockBuilderFactory: Send + Sync {
    fn create_block_builder(&self) -> Box<dyn BlockBuilderTrait>;
}

pub(crate) struct BlockBuilderFactoryImpl {}

impl BlockBuilderFactory for BlockBuilderFactoryImpl {
    fn create_block_builder(&self) -> Box<dyn BlockBuilderTrait> {
        // TODO: Implement.
        unimplemented!()
    }
}

#[allow(dead_code)]
struct ProposalGenerationTask {
    pub timeout: tokio::time::Instant,
    pub mempool_client: SharedMempoolClient,
    pub max_txs_per_mempool_request: usize,
    pub sender: tokio::sync::mpsc::Sender<Transaction>,
    pub proposal_in_generation: Arc<Mutex<Option<ProposalId>>>,
    pub block_builder_factory: Arc<dyn BlockBuilderFactory>,
}

impl ProposalGenerationTask {
    #[allow(dead_code)]
    async fn run(self) -> ProposalsManagerResult<()> {
        let block_builder = self.block_builder_factory.create_block_builder();
        loop {
            if tokio::time::Instant::now() > self.timeout {
                info!("Proposal reached timeout.");
                break;
            }
            let mempool_txs = self.mempool_client.get_txs(self.max_txs_per_mempool_request).await?;
            if mempool_txs.is_empty() {
                // TODO: check if sleep is needed here.
                tokio::task::yield_now().await;
                continue;
            }

            // TODO: Get L1 transactions.
            debug!("Adding {} mempool transactions to proposal in generation.", mempool_txs.len());
            // TODO: This is cpu bound operation, should use spawn_blocking / Rayon / std::thread
            // here or from inside the function.
            let is_block_ready = block_builder.add_txs(mempool_txs.as_slice());
            for tx in mempool_txs {
                debug!("Sending tx {}.", tx.tx_hash());
                if self.sender.send(tx).await.is_err() {
                    error!("Failed to send tx to the receiver.");
                    return Err(ProposalsManagerError::InternalError);
                }
            }
            if is_block_ready {
                break;
            }
        }

        info!("Closing block.");
        // TODO: Get state diff.
        let mut proposal_id = self.proposal_in_generation.lock().await;
        *proposal_id = None;

        Ok(())
    }
}
