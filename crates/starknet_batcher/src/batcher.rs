use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use blockifier::abi::constants;
use blockifier::state::global_cache::GlobalContractCache;
use chrono::Utc;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    BatcherResult,
    DecisionReachedInput,
    GetHeightResponse,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::CommitBlockArgs;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tracing::{debug, error, info, instrument, trace};

use crate::block_builder::{
    BlockBuilderError,
    BlockBuilderExecutionParams,
    BlockBuilderFactory,
    BlockBuilderFactoryTrait,
    BlockMetadata,
};
use crate::config::BatcherConfig;
use crate::proposal_manager::{
    GenerateProposalError,
    InternalProposalStatus,
    ProposalError,
    ProposalManager,
    ProposalManagerTrait,
    ProposalOutput,
};
use crate::transaction_provider::{
    DummyL1ProviderClient,
    ProposeTransactionProvider,
    ValidateTransactionProvider,
};

type OutputStreamReceiver = tokio::sync::mpsc::UnboundedReceiver<Transaction>;
type InputStreamSender = tokio::sync::mpsc::Sender<Transaction>;

pub struct Batcher {
    pub config: BatcherConfig,
    pub storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    pub storage_writer: Box<dyn BatcherStorageWriterTrait>,
    pub mempool_client: SharedMempoolClient,

    active_height: Option<BlockNumber>,
    proposal_manager: Box<dyn ProposalManagerTrait>,

    block_builder_factory: Box<dyn BlockBuilderFactoryTrait>,
    propose_tx_streams: HashMap<ProposalId, OutputStreamReceiver>,
    validate_tx_streams: HashMap<ProposalId, InputStreamSender>,
}

impl Batcher {
    pub(crate) fn new(
        config: BatcherConfig,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
        storage_writer: Box<dyn BatcherStorageWriterTrait>,
        mempool_client: SharedMempoolClient,
        block_builder_factory: Box<dyn BlockBuilderFactoryTrait>,
        proposal_manager: Box<dyn ProposalManagerTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            storage_reader,
            storage_writer,
            mempool_client,
            active_height: None,
            block_builder_factory,
            proposal_manager,
            propose_tx_streams: HashMap::new(),
            validate_tx_streams: HashMap::new(),
        }
    }

    #[instrument(skip(self), err)]
    pub async fn start_height(&mut self, input: StartHeightInput) -> BatcherResult<()> {
        if self.active_height == Some(input.height) {
            return Err(BatcherError::HeightInProgress);
        }

        let storage_height =
            self.storage_reader.height().map_err(|_| BatcherError::InternalError)?;
        if storage_height < input.height {
            return Err(BatcherError::StorageNotSynced {
                storage_height,
                requested_height: input.height,
            });
        }
        if storage_height > input.height {
            return Err(BatcherError::HeightAlreadyPassed {
                storage_height,
                requested_height: input.height,
            });
        }

        // Clear all the proposals from the previous height.
        self.proposal_manager.reset().await;
        self.propose_tx_streams.clear();
        self.validate_tx_streams.clear();

        info!("Starting to work on height {}.", input.height);
        self.active_height = Some(input.height);

        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn propose_block(
        &mut self,
        propose_block_input: ProposeBlockInput,
    ) -> BatcherResult<()> {
        let active_height = self.active_height.ok_or(BatcherError::NoActiveHeight)?;
        verify_block_input(
            active_height,
            propose_block_input.block_info.block_number,
            propose_block_input.retrospective_block_hash,
        )?;

        let tx_provider = ProposeTransactionProvider::new(
            self.mempool_client.clone(),
            // TODO: use a real L1 provider client.
            Arc::new(DummyL1ProviderClient),
            self.config.max_l1_handler_txs_per_block_proposal,
        );

        // A channel to receive the transactions included in the proposed block.
        let (output_tx_sender, output_tx_receiver) = tokio::sync::mpsc::unbounded_channel();

        let (block_builder, abort_signal_sender) = self
            .block_builder_factory
            .create_block_builder(
                BlockMetadata {
                    block_info: propose_block_input.block_info,
                    retrospective_block_hash: propose_block_input.retrospective_block_hash,
                },
                BlockBuilderExecutionParams {
                    deadline: deadline_as_instant(propose_block_input.deadline)?,
                    fail_on_err: false,
                },
                Box::new(tx_provider),
                Some(output_tx_sender),
            )
            .map_err(|_| BatcherError::InternalError)?;

        self.proposal_manager
            .spawn_proposal(propose_block_input.proposal_id, block_builder, abort_signal_sender)
            .await?;

        self.propose_tx_streams.insert(propose_block_input.proposal_id, output_tx_receiver);
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn validate_block(
        &mut self,
        validate_block_input: ValidateBlockInput,
    ) -> BatcherResult<()> {
        let active_height = self.active_height.ok_or(BatcherError::NoActiveHeight)?;
        verify_block_input(
            active_height,
            validate_block_input.block_info.block_number,
            validate_block_input.retrospective_block_hash,
        )?;

        // A channel to send the transactions to include in the block being validated.
        let (input_tx_sender, input_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.input_stream_content_buffer_size);

        let tx_provider = ValidateTransactionProvider {
            tx_receiver: input_tx_receiver,
            // TODO: use a real L1 provider client.
            l1_provider_client: Arc::new(DummyL1ProviderClient),
        };

        let (block_builder, abort_signal_sender) = self
            .block_builder_factory
            .create_block_builder(
                BlockMetadata {
                    block_info: validate_block_input.block_info,
                    retrospective_block_hash: validate_block_input.retrospective_block_hash,
                },
                BlockBuilderExecutionParams {
                    deadline: deadline_as_instant(validate_block_input.deadline)?,
                    fail_on_err: true,
                },
                Box::new(tx_provider),
                None,
            )
            .map_err(|_| BatcherError::InternalError)?;

        self.proposal_manager
            .spawn_proposal(validate_block_input.proposal_id, block_builder, abort_signal_sender)
            .await?;

        self.validate_tx_streams.insert(validate_block_input.proposal_id, input_tx_sender);
        Ok(())
    }

    // This function assumes that requests are received in order, otherwise the content could
    // be processed out of order.
    #[instrument(skip(self), err)]
    pub async fn send_proposal_content(
        &mut self,
        send_proposal_content_input: SendProposalContentInput,
    ) -> BatcherResult<SendProposalContentResponse> {
        // TODO(Dafna): this method should return an meaningful error if the given proposal_id
        // is of a proposed block (and not validated). Currently it panics or returns a
        // wrong error.

        let proposal_id = send_proposal_content_input.proposal_id;

        match send_proposal_content_input.content {
            SendProposalContent::Txs(txs) => self.send_txs_and_get_status(proposal_id, txs).await,
            SendProposalContent::Finish => {
                self.close_tx_channel_and_get_commitment(proposal_id).await
            }
            SendProposalContent::Abort => {
                self.proposal_manager.abort_proposal(proposal_id).await;
                Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
            }
        }
    }

    async fn send_txs_and_get_status(
        &mut self,
        proposal_id: ProposalId,
        txs: Vec<Transaction>,
    ) -> BatcherResult<SendProposalContentResponse> {
        match self.proposal_manager.get_proposal_status(proposal_id).await {
            InternalProposalStatus::Processing => {
                let tx_provider_sender = &self
                    .validate_tx_streams
                    .get(&proposal_id)
                    .expect("Expecting tx_provider_sender to exist during batching.");
                for tx in txs {
                    tx_provider_sender.send(tx).await.map_err(|err| {
                        error!("Failed to send transaction to the tx provider: {}", err);
                        BatcherError::InternalError
                    })?;
                }
                Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
            }
            // Proposal Got an Error while processing transactions.
            InternalProposalStatus::Failed => {
                Ok(SendProposalContentResponse { response: ProposalStatus::InvalidProposal })
            }
            InternalProposalStatus::Finished => {
                Err(BatcherError::ProposalAlreadyFinished { proposal_id })
            }
            InternalProposalStatus::NotFound => Err(BatcherError::ProposalNotFound { proposal_id }),
        }
    }

    async fn close_tx_channel_and_get_commitment(
        &mut self,
        proposal_id: ProposalId,
    ) -> BatcherResult<SendProposalContentResponse> {
        debug!("Send proposal content done for {}", proposal_id);

        self.close_input_transaction_stream(proposal_id)?;

        let response = match self.proposal_manager.await_proposal_commitment(proposal_id).await {
            Some(Ok(proposal_commitment)) => ProposalStatus::Finished(proposal_commitment),
            Some(Err(ProposalError::BlockBuilderError(err))) => match err.as_ref() {
                BlockBuilderError::FailOnError(_) => ProposalStatus::InvalidProposal,
                _ => return Err(BatcherError::InternalError),
            },
            Some(Err(ProposalError::Aborted)) => return Err(BatcherError::ProposalAborted),
            None => {
                panic!("Proposal {} should exist in the proposal manager.", proposal_id)
            }
        };

        Ok(SendProposalContentResponse { response })
    }

    fn close_input_transaction_stream(&mut self, proposal_id: ProposalId) -> BatcherResult<()> {
        self.validate_tx_streams
            .remove(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?;
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn get_height(&mut self) -> BatcherResult<GetHeightResponse> {
        let height = self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })?;
        Ok(GetHeightResponse { height })
    }

    #[instrument(skip(self), err)]
    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let proposal_id = get_proposal_content_input.proposal_id;

        let tx_stream = &mut self
            .propose_tx_streams
            .get_mut(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?;

        // Blocking until we have some txs to stream or the proposal is done.
        let mut txs = Vec::new();
        let n_executed_txs =
            tx_stream.recv_many(&mut txs, self.config.outstream_content_buffer_size).await;

        if n_executed_txs != 0 {
            debug!("Streaming {} txs", n_executed_txs);
            return Ok(GetProposalContentResponse { content: GetProposalContent::Txs(txs) });
        }

        // Finished streaming all the transactions.
        // TODO: Consider removing the proposal from the proposal manager and keep it in the batcher
        // for decision reached.
        self.propose_tx_streams.remove(&proposal_id);
        let proposal_commitment = self
            .proposal_manager
            .await_proposal_commitment(proposal_id)
            .await
            .ok_or(BatcherError::ProposalNotFound { proposal_id })??;
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(proposal_commitment),
        })
    }

    pub async fn add_sync_block(&mut self, sync_block: SyncBlock) -> BatcherResult<()> {
        let SyncBlock { state_diff, transaction_hashes } = sync_block;
        let address_to_nonce = state_diff.nonces.iter().map(|(k, v)| (*k, *v)).collect();
        let tx_hashes = transaction_hashes.into_iter().collect();
        // TODO: Keep the height from start_height or get it from the input.
        let height = self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })?;
        info!("Syncing block at height {} and notifying mempool of the block.", height);
        self.commit_proposal_and_block(height, state_diff, address_to_nonce, tx_hashes).await
    }

    #[instrument(skip(self), err)]
    pub async fn decision_reached(&mut self, input: DecisionReachedInput) -> BatcherResult<()> {
        let proposal_id = input.proposal_id;
        let proposal_output = self
            .proposal_manager
            .take_proposal_result(proposal_id)
            .await
            .ok_or(BatcherError::ExecutedProposalNotFound { proposal_id })??;
        let ProposalOutput { state_diff, nonces: address_to_nonce, tx_hashes, .. } =
            proposal_output;
        // TODO(Arni): Move the height and the `info` level log into the function
        // `commit_proposal_and_block`.
        // TODO: Keep the height from start_height or get it from the input.
        let height = self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })?;
        info!(
            "Committing proposal {} at height {} and notifying mempool of the block.",
            proposal_id, height
        );
        self.commit_proposal_and_block(height, state_diff, address_to_nonce, tx_hashes).await
    }

    async fn commit_proposal_and_block(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        address_to_nonce: HashMap<ContractAddress, Nonce>,
        tx_hashes: HashSet<TransactionHash>,
    ) -> BatcherResult<()> {
        trace!("Transactions: {:#?}, State diff: {:#?}.", tx_hashes, state_diff);
        self.storage_writer.commit_proposal(height, state_diff).map_err(|err| {
            error!("Failed to commit proposal to storage: {}", err);
            BatcherError::InternalError
        })?;
        if let Err(mempool_err) =
            self.mempool_client.commit_block(CommitBlockArgs { address_to_nonce, tx_hashes }).await
        {
            error!("Failed to commit block to mempool: {}", mempool_err);
            // TODO: Should we rollback the state diff and return an error?
        }
        Ok(())
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");

    let block_builder_factory = Box::new(BlockBuilderFactory {
        block_builder_config: config.block_builder_config.clone(),
        storage_reader: storage_reader.clone(),
        global_class_hash_to_class: GlobalContractCache::new(config.global_contract_cache_size),
    });
    let storage_reader = Arc::new(storage_reader);
    let storage_writer = Box::new(storage_writer);
    let proposal_manager = Box::new(ProposalManager::new());
    Batcher::new(
        config,
        storage_reader,
        storage_writer,
        mempool_client,
        block_builder_factory,
        proposal_manager,
    )
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {
    /// Returns the next height that the batcher should work on.
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber>;
}

impl BatcherStorageReaderTrait for papyrus_storage::StorageReader {
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_state_marker()
    }
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageWriterTrait: Send + Sync {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
    ) -> papyrus_storage::StorageResult<()>;
}

impl BatcherStorageWriterTrait for papyrus_storage::StorageWriter {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
    ) -> papyrus_storage::StorageResult<()> {
        // TODO: write casms.
        self.begin_rw_txn()?.append_state_diff(height, state_diff)?.commit()
    }
}

impl From<GenerateProposalError> for BatcherError {
    fn from(err: GenerateProposalError) -> Self {
        match err {
            GenerateProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id,
            } => BatcherError::ServerBusy {
                active_proposal_id: current_generating_proposal_id,
                new_proposal_id,
            },
            GenerateProposalError::BlockBuilderError(..) => BatcherError::InternalError,
            GenerateProposalError::NoActiveHeight => BatcherError::NoActiveHeight,
            GenerateProposalError::ProposalAlreadyExists { proposal_id } => {
                BatcherError::ProposalAlreadyExists { proposal_id }
            }
        }
    }
}

impl From<ProposalError> for BatcherError {
    fn from(err: ProposalError) -> Self {
        match err {
            ProposalError::BlockBuilderError(..) => BatcherError::InternalError,
            ProposalError::Aborted => BatcherError::ProposalAborted,
        }
    }
}

impl ComponentStarter for Batcher {}

pub fn deadline_as_instant(deadline: chrono::DateTime<Utc>) -> BatcherResult<tokio::time::Instant> {
    let time_to_deadline = deadline - chrono::Utc::now();
    let as_duration =
        time_to_deadline.to_std().map_err(|_| BatcherError::TimeToDeadlineError { deadline })?;
    // TODO(Matan): this is a temporary solution to the timeout issue.
    Ok((std::time::Instant::now() + (as_duration / 2)).into())
}

fn verify_block_input(
    height: BlockNumber,
    block_number: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    verify_non_empty_retrospective_block_hash(height, retrospective_block_hash)?;
    verify_block_number(height, block_number)?;
    Ok(())
}

fn verify_non_empty_retrospective_block_hash(
    height: BlockNumber,
    retrospective_block_hash: Option<BlockHashAndNumber>,
) -> BatcherResult<()> {
    if height >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER)
        && retrospective_block_hash.is_none()
    {
        return Err(BatcherError::MissingRetrospectiveBlockHash);
    }
    Ok(())
}

fn verify_block_number(height: BlockNumber, block_number: BlockNumber) -> BatcherResult<()> {
    if block_number != height {
        return Err(BatcherError::InvalidBlockNumber { active_height: height, block_number });
    }
    Ok(())
}
