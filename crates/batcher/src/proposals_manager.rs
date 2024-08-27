use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::StreamExt;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_batcher_types::batcher_types::ProposalContentId;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::sync::Mutex;
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
    #[error("No closed block for height {height} with content id {content_id:?}.")]
    ClosedBlockNotFound { height: BlockNumber, content_id: ProposalContentId },
    #[error("Internal error.")]
    InternalError,
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error(transparent)]
    PapyrusStorageError(#[from] papyrus_storage::StorageError),
}

pub type ProposalsManagerResult<T> = Result<T, ProposalsManagerError>;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ProposalsManagerTrait: Send + Sync {
    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    async fn generate_block_proposal<'a>(
        &mut self,
        timeout: tokio::time::Instant,
        height: BlockNumber,
    ) -> ProposalsManagerResult<futures::stream::BoxStream<'a, Transaction>>;

    async fn decision_reached(
        &mut self,
        height: BlockNumber,
        content_id: ProposalContentId,
    ) -> ProposalsManagerResult<()>;
}

#[derive(Clone, Debug, Default)]
pub struct ClosedBlock {
    pub content_id: ProposalContentId,
    pub height: BlockNumber,
    pub header: BlockHeader,
    pub state_diff: ThinStateDiff,
    pub compiled_classes: Vec<(ClassHash, CasmContractClass)>,
}

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
    proposal_in_generation: Arc<Mutex<Option<(ProposalId, BlockNumber)>>>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactory>,
    proposal_id_marker: ProposalId,
    closed_proposals: HashMap<BlockNumber, Arc<Mutex<Vec<ClosedBlock>>>>,
    active_proposal_handle: Option<tokio::task::JoinHandle<ProposalsManagerResult<()>>>,
    storage_writer: Arc<Mutex<dyn StorageWriterTrait>>,
}

impl ProposalsManager {
    // TODO: Remove dead_code attribute.
    #[allow(dead_code)]
    pub fn new(
        config: ProposalsManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactory>,
        storage_writer: Arc<Mutex<dyn StorageWriterTrait>>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            proposal_in_generation: Arc::new(Mutex::new(None)),
            block_builder_factory,
            proposal_id_marker: ProposalId::default(),
            closed_proposals: HashMap::new(),
            active_proposal_handle: None,
            storage_writer,
        }
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_proposal_in_generation(
        &mut self,
        proposal_id: ProposalId,
        height: BlockNumber,
    ) -> ProposalsManagerResult<()> {
        let mut lock = self.proposal_in_generation.lock().await;

        if let Some(proposal_in_generation) = *lock {
            return Err(ProposalsManagerError::AlreadyGeneratingProposal {
                current_generating_proposal_id: proposal_in_generation.0,
                new_proposal_id: proposal_id,
            });
        }

        *lock = Some((proposal_id, height));
        debug!("Set proposal {} as the one being generated.", proposal_id);
        Ok(())
    }

    async fn abort_active_proposal_if_needed(&mut self, height_to_check: BlockNumber) {
        let mut maybe_active_proposal = self.proposal_in_generation.lock().await;
        if let Some((active_proposal_id, active_proposal_height)) = *maybe_active_proposal {
            if active_proposal_height == height_to_check {
                info!(
                    "Aborting active proposal {} because another proposal was chosen.",
                    active_proposal_id
                );
                *maybe_active_proposal = None;
                if let Some(handle) = self.active_proposal_handle.take() {
                    handle.abort();
                } else {
                    error!("Tried to abort the active proposal but the handle is None.");
                    // TODO: Consider returning internal error.
                }
            }
        }
    }
}

#[async_trait]
impl ProposalsManagerTrait for ProposalsManager {
    #[instrument(skip(self), fields(proposal_id))]
    async fn generate_block_proposal<'a>(
        &mut self,
        timeout: tokio::time::Instant,
        height: BlockNumber,
    ) -> ProposalsManagerResult<futures::stream::BoxStream<'a, Transaction>> {
        let proposal_id = self.proposal_id_marker;
        self.proposal_id_marker += 1;
        info!("Starting generation of new proposal.");
        self.set_proposal_in_generation(proposal_id, height).await?;

        let (sender, receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.config.outstream_content_buffer_size);
        // TODO: Find where to join the task - needed to make sure it starts immediatly.
        self.active_proposal_handle = Some(tokio::spawn(
            ProposalGenerationTask {
                timeout,
                mempool_client: self.mempool_client.clone(),
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                sender: Arc::new(sender),
                proposal_in_generation: self.proposal_in_generation.clone(),
                block_builder_factory: self.block_builder_factory.clone(),
                closed_proposals_at_height: self
                    .closed_proposals
                    .entry(height)
                    .or_default()
                    .clone(),
            }
            .run()
            .in_current_span(),
        ));

        Ok(ReceiverStream::new(receiver).boxed())
    }

    #[instrument(skip(self))]
    async fn decision_reached(
        &mut self,
        height: BlockNumber,
        content_id: ProposalContentId,
    ) -> ProposalsManagerResult<()> {
        info!("Decision reached, choosing proposal with content id {}.", content_id);
        self.abort_active_proposal_if_needed(height).await;

        let closed_proposals_at_height = self
            .closed_proposals
            .remove(&height)
            .ok_or(ProposalsManagerError::ClosedBlockNotFound { height, content_id })?;

        // Unwrap the Arc and Mutex in order to be able to move the content into the storage update
        // function. At this point, the proposal is already closed and we should be able to safely
        // unwrap the Arc and Mutex as we are the only owners of the Arc.
        let closed_proposals = Arc::try_unwrap(closed_proposals_at_height)
            .map_err(|_closed_proposals_at_height| {
                error!("Failed to unwrap closed_proposals_at_height.");
                // TODO: Consider returning the proposals to the map so we might be able to retry.
                ProposalsManagerError::InternalError
            })?
            .into_inner();

        let chosen_block = closed_proposals
            .into_iter()
            .find(|closed_block| closed_block.content_id == content_id)
            .ok_or(ProposalsManagerError::ClosedBlockNotFound { height, content_id })?;

        self.storage_writer.lock().await.commit_block(chosen_block)?;
        // The rest of the proposals are dropped as they were not chosen.
        Ok(())
    }
}

#[async_trait]
pub trait BlockBuilderTrait: Send {
    /// Returning ClosedBlock if the block is ready to be proposed.
    async fn add_txs_and_stream(
        &self,
        txs: Vec<Transaction>,
        sender: Arc<tokio::sync::mpsc::Sender<Transaction>>,
    ) -> Option<ClosedBlock>;
}

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

struct ProposalGenerationTask {
    pub timeout: tokio::time::Instant,
    pub mempool_client: SharedMempoolClient,
    pub max_txs_per_mempool_request: usize,
    pub sender: Arc<tokio::sync::mpsc::Sender<Transaction>>,
    pub proposal_in_generation: Arc<Mutex<Option<(ProposalId, BlockNumber)>>>,
    pub block_builder_factory: Arc<dyn BlockBuilderFactory>,
    pub closed_proposals_at_height: Arc<Mutex<Vec<ClosedBlock>>>,
}

impl ProposalGenerationTask {
    async fn run(self) -> ProposalsManagerResult<()> {
        let block_builder = self.block_builder_factory.create_block_builder();
        let mut closed_block = None;
        while closed_block.is_none() {
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
            closed_block = block_builder.add_txs_and_stream(mempool_txs, self.sender.clone()).await;
        }

        info!("Closing block.");
        self.closed_proposals_at_height
            .lock()
            .await
            .push(closed_block.expect("Expected closed block."));
        let mut proposal_id = self.proposal_in_generation.lock().await;
        *proposal_id = None;

        Ok(())
    }
}

#[cfg_attr(test, automock)]
pub trait StorageWriterTrait: Send + Sync {
    fn commit_block(&mut self, block: ClosedBlock) -> ProposalsManagerResult<()>;
}

impl StorageWriterTrait for papyrus_storage::StorageWriter {
    fn commit_block(&mut self, block: ClosedBlock) -> ProposalsManagerResult<()> {
        let mut tx = self
            .begin_rw_txn()?
            .append_header(block.height, &block.header)?
            .append_state_diff(block.height, block.state_diff)?;
        for (class_hash, casm) in block.compiled_classes {
            tx = tx.append_casm(&class_hash, &casm)?;
        }
        tx.commit()?;
        Ok(())
    }
}
