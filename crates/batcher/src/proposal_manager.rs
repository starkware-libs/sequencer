use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_batcher_types::batcher_types::{GetProposalContent, ProposalId};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tokio::select;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, instrument, trace, Instrument};

use crate::batcher::{BatcherStorageReaderTrait, BatcherStorageWriterTrait};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
                 blocking further filling of the stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_txs_per_mempool_request",
                &self.max_txs_per_mempool_request,
                "Maximum transactions to get from the mempool in a single get_txs request.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "outstream_content_buffer_size",
                &self.outstream_content_buffer_size,
                "Maximum items to add to the outstream buffer before blocking further filling of \
                 the stream",
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
    MempoolError(#[from] MempoolClientError),
    #[error("No active height to work on.")]
    NoActiveHeight,
    #[error("Proposal with ID {proposal_id} already exists.")]
    ProposalAlreadyExists { proposal_id: ProposalId },
}

#[derive(Debug, Error)]
pub enum GetProposalContentError {
    #[error("Can't get content for proposal with ID {proposal_id} as it is not a build proposal.")]
    GetContentOnNonBuildProposal { proposal_id: ProposalId },
    #[error("Proposal with ID {proposal_id} not found.")]
    ProposalNotFound { proposal_id: ProposalId },
    #[error("Stream exhausted.")]
    StreamExhausted,
}

#[derive(Debug, Error)]
pub enum DecisionReachedError {
    #[error(transparent)]
    BuildProposalError(#[from] BuildProposalError),
    #[error("Proposal {proposal_id} is not done yet.")]
    ProposalNotDone { proposal_id: ProposalId },
    #[error("Decision reached for proposal with ID {proposal_id} that does not exist.")]
    ProposalNotFound { proposal_id: ProposalId },
    #[error(transparent)]
    StorageError(#[from] papyrus_storage::StorageError),
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
    storage_writer: Box<dyn BatcherStorageWriterTrait>,
    active_height: Option<BlockNumber>,
    /// The block proposal that is currently being proposed, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ActiveProposal>>>,
    // Use a factory object, to be able to mock BlockBuilder in tests.
    block_builder_factory: Arc<dyn BlockBuilderFactoryTrait>,
    // The list of all proposals that were generated in the current height.
    proposals: Arc<Mutex<HashMap<ProposalId, Proposal>>>,
}

pub struct Proposal {
    content_stream: ProposalContentStream,
    // Set to some when the BlockBuilderTask is done.
    block_builder_result: Option<Result<BlockBuilderOutput, BuildProposalError>>,
}

struct ActiveProposal {
    pub proposal_id: ProposalId,
    // Used to abort the block builder.
    pub abort: tokio::sync::oneshot::Sender<()>,
}

impl ActiveProposal {
    pub fn abort(self) {
        self.abort.send(()).expect("Expecting abort channel to be open.");
    }
}

impl ProposalManager {
    pub fn new(
        config: ProposalManagerConfig,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Arc<dyn BlockBuilderFactoryTrait>,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
        storage_writer: Box<dyn BatcherStorageWriterTrait>,
    ) -> Self {
        Self {
            config,
            mempool_client,
            storage_reader,
            storage_writer,
            active_proposal: Arc::new(Mutex::new(None)),
            block_builder_factory,
            active_height: None,
            proposals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts working on the given height.
    #[instrument(skip(self), err)]
    pub fn start_height(&mut self, height: BlockNumber) -> Result<(), StartHeightError> {
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
        info!("Starting to work on height {}.", height);
        self.active_height = Some(height);
        Ok(())
    }

    /// Starts a new block proposal generation task for the given proposal_id and height with
    /// transactions from the mempool.
    /// Requires output_content_sender for sending the generated transactions to the caller.
    #[instrument(skip(self), err, fields(self.active_height))]
    pub async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
    ) -> Result<(), BuildProposalError> {
        if self.active_height.is_none() {
            return Err(BuildProposalError::NoActiveHeight);
        }
        if self.proposals.lock().await.contains_key(&proposal_id) {
            return Err(BuildProposalError::ProposalAlreadyExists { proposal_id });
        }
        info!("Starting generation of a new proposal with id {}.", proposal_id);
        // TODO: Should this be an unbounded channel?
        let (output_content_sender, output_content_receiver) =
            tokio::sync::mpsc::channel(self.config.outstream_content_buffer_size);
        let content_stream =
            ProposalContentStream::BuildProposal(OutputStream::new(output_content_receiver));

        let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
        self.set_active_proposal(ActiveProposal { proposal_id, abort: abort_tx }).await?;
        self.proposals
            .lock()
            .await
            .insert(proposal_id, Proposal { content_stream, block_builder_result: None });

        let block_builder = self.block_builder_factory.create_block_builder();

        tokio::spawn(
            BuildProposalTask {
                proposal_id,
                mempool_client: self.mempool_client.clone(),
                output_content_sender,
                block_builder_next_txs_buffer_size: self.config.block_builder_next_txs_buffer_size,
                max_txs_per_mempool_request: self.config.max_txs_per_mempool_request,
                block_builder,
                active_proposal: self.active_proposal.clone(),
                deadline,
                proposals: self.proposals.clone(),
                abort: abort_rx,
            }
            .run()
            .in_current_span(),
        );
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn get_proposal_content(
        &mut self,
        proposal_id: ProposalId,
    ) -> Result<GetProposalContent, GetProposalContentError> {
        let mut proposals = self.proposals.lock().await;
        let proposal = proposals
            .get_mut(&proposal_id)
            .ok_or(GetProposalContentError::ProposalNotFound { proposal_id })?;

        let ProposalContentStream::BuildProposal(content_stream) = &mut proposal.content_stream
        else {
            return Err(GetProposalContentError::GetContentOnNonBuildProposal { proposal_id });
        };
        content_stream.next().await.ok_or(GetProposalContentError::StreamExhausted)
    }

    #[instrument(skip(self), err)]
    pub async fn decision_reached(
        &mut self,
        proposal_id: ProposalId,
    ) -> Result<(), DecisionReachedError> {
        let proposal = {
            let mut proposals = self.proposals.lock().await;
            proposals
                .remove(&proposal_id)
                .ok_or(DecisionReachedError::ProposalNotFound { proposal_id })?
        };

        let BlockBuilderOutput::Done { state_diff, casms } = proposal
            .block_builder_result
            .ok_or(DecisionReachedError::ProposalNotDone { proposal_id })??
        else {
            panic!("Block builder unexpectedly aborted.");
        };

        info!("Committing proposal with id {}.", proposal_id);
        self.storage_writer.commit_proposal(
            self.active_height.expect("Expecting active height."),
            state_diff,
            &casms,
        )?;
        self.reset_height().await?;
        Ok(())
    }

    async fn reset_height(&mut self) -> Result<(), BuildProposalError> {
        if let Some(active_proposal) = self.active_proposal.lock().await.take() {
            debug!("Aborting the active proposal: {}.", active_proposal.proposal_id);
            active_proposal.abort();
        }
        self.proposals.lock().await.clear();
        self.active_height = None;
        Ok(())
    }

    // Checks if there is already a proposal being generated, and if not, sets the given proposal_id
    // as the one being generated.
    async fn set_active_proposal(
        &mut self,
        active_proposal: ActiveProposal,
    ) -> Result<(), BuildProposalError> {
        let proposal_id = active_proposal.proposal_id;
        let mut lock = self.active_proposal.lock().await;

        if let Some(current_generating_proposal_id) =
            lock.as_ref().map(|already_active_proposal| already_active_proposal.proposal_id)
        {
            return Err(BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id: proposal_id,
            });
        }

        *lock = Some(active_proposal);
        debug!("Set proposal {} as the one being generated.", proposal_id);
        Ok(())
    }
}

pub(crate) enum ProposalContentStream {
    BuildProposal(OutputStream),
    // TODO: Add stream.
    #[allow(dead_code)]
    ValidateProposal,
}
// TODO: Make this a fuse stream to make sure it always returns None when exhausted.
type OutputStream = tokio_stream::wrappers::ReceiverStream<GetProposalContent>;

struct BuildProposalTask {
    proposal_id: ProposalId,
    mempool_client: SharedMempoolClient,
    output_content_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    max_txs_per_mempool_request: usize,
    block_builder_next_txs_buffer_size: usize,
    block_builder: Arc<dyn BlockBuilderTrait>,
    active_proposal: Arc<Mutex<Option<ActiveProposal>>>,
    deadline: tokio::time::Instant,
    proposals: Arc<Mutex<HashMap<ProposalId, Proposal>>>,
    abort: tokio::sync::oneshot::Receiver<()>,
}

impl BuildProposalTask {
    async fn run(self) -> Result<(), BuildProposalError> {
        // We convert the receiver to a stream and pass it to the block builder while using the
        // sender to feed the stream.
        let (mempool_tx_sender, mempool_tx_receiver) =
            tokio::sync::mpsc::channel::<Transaction>(self.block_builder_next_txs_buffer_size);
        let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);
        let building_future = self.block_builder.build_block(
            self.deadline,
            mempool_tx_stream,
            self.output_content_sender.clone(),
        );

        let feed_mempool_txs_future = Self::feed_mempool_txs(
            &self.mempool_client,
            self.max_txs_per_mempool_request,
            &mempool_tx_sender,
        );

        let abort_listener = self.abort;

        // Keep the necessary fields before self is consumed.
        let proposal_id = self.proposal_id;
        let active_proposal = self.active_proposal.clone();
        let proposals = self.proposals.clone();

        // Wait for either the block builder to finish / the feeding of transactions to error / the
        // proposal to be aborted. The other tasks will be cancelled.
        let block_builder_result = select! {
            // This will send txs from the mempool to the stream we provided to the block builder.
            feeding_error = feed_mempool_txs_future => {
                error!("Failed to feed more mempool txs: {}.", feeding_error);
                // TODO: Notify the mempool about remaining txs.
                self.block_builder.abort_build();
                Err(feeding_error)
            },
            builder_done = building_future => {
                info!("Block builder finished.");
                Ok(builder_done)
            }
            _ = abort_listener => {
                info!("Proposal aborted, aborting block builder.");
                Ok(self.block_builder.abort_build())
            }
        };
        Self::active_proposal_finished(
            proposal_id,
            active_proposal,
            block_builder_result,
            proposals,
        )
        .await;
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

    async fn active_proposal_finished(
        proposal_id: ProposalId,
        active_proposal: Arc<Mutex<Option<ActiveProposal>>>,
        res: Result<BlockBuilderOutput, BuildProposalError>,
        proposals: Arc<Mutex<HashMap<ProposalId, Proposal>>>,
    ) {
        *active_proposal.lock().await = None;
        proposals
            .lock()
            .await
            .get_mut(&proposal_id)
            .expect("Expecting proposal to exist.")
            .block_builder_result = Some(res);
    }
}

pub type InputTxStream = ReceiverStream<Transaction>;

// TODO: Move to the block builder module.
#[derive(Clone, Debug)]
pub enum BlockBuilderOutput {
    Done {
        state_diff: ThinStateDiff,
        casms: Vec<(ClassHash, CasmContractClass)>,
    },
    #[allow(dead_code)]
    Aborted,
}

impl Default for BlockBuilderOutput {
    fn default() -> Self {
        Self::Done { state_diff: ThinStateDiff::default(), casms: Vec::new() }
    }
}

#[async_trait]
pub trait BlockBuilderTrait: Send + Sync {
    async fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    ) -> BlockBuilderOutput;

    fn abort_build(&self) -> BlockBuilderOutput;
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderFactoryTrait: Send + Sync {
    fn create_block_builder(&self) -> Arc<dyn BlockBuilderTrait>;
}

pub(crate) struct BlockBuilderFactory {}

impl BlockBuilderFactoryTrait for BlockBuilderFactory {
    fn create_block_builder(&self) -> Arc<dyn BlockBuilderTrait> {
        // TODO: Implement.
        unimplemented!()
    }
}
