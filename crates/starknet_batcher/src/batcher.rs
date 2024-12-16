use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use blockifier::state::global_cache::GlobalContractCache;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use starknet_api::block::BlockNumber;
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
    ProposalCommitment,
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
    BlockBuilderExecutionParams,
    BlockBuilderFactory,
    BlockBuilderFactoryTrait,
    BlockMetadata,
};
use crate::config::BatcherConfig;
use crate::proposal_manager::{GenerateProposalError, ProposalManager, ProposalManagerTrait};
use crate::transaction_provider::{
    DummyL1ProviderClient,
    ProposeTransactionProvider,
    ValidateTransactionProvider,
};
use crate::utils::{
    deadline_as_instant,
    proposal_status_from,
    verify_block_input,
    ProposalOutput,
    ProposalResult,
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

        let storage_height = self.get_height_from_storage()?;
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

        self.abort_active_height().await;

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
        let proposal_id = send_proposal_content_input.proposal_id;
        if !self.validate_tx_streams.contains_key(&proposal_id) {
            return Err(BatcherError::ProposalNotFound { proposal_id });
        }

        match send_proposal_content_input.content {
            SendProposalContent::Txs(txs) => self.handle_send_txs_request(proposal_id, txs).await,
            SendProposalContent::Finish => self.handle_finish_proposal_request(proposal_id).await,
            SendProposalContent::Abort => self.handle_abort_proposal_request(proposal_id).await,
        }
    }

    /// Clear all the proposals from the previous height.
    async fn abort_active_height(&mut self) {
        self.proposal_manager.reset().await;
        self.propose_tx_streams.clear();
        self.validate_tx_streams.clear();
    }

    async fn handle_send_txs_request(
        &mut self,
        proposal_id: ProposalId,
        txs: Vec<Transaction>,
    ) -> BatcherResult<SendProposalContentResponse> {
        if self.is_active(proposal_id).await {
            //   The proposal is active. Send the transactions through the tx provider.
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
            return Ok(SendProposalContentResponse { response: ProposalStatus::Processing });
        }

        // The proposal is no longer active, can't send the transactions.
        let proposal_result =
            self.get_completed_proposal_result(proposal_id).await.expect("Proposal should exist.");
        match proposal_result {
            Ok(_) => Err(BatcherError::ProposalAlreadyFinished { proposal_id }),
            Err(err) => Ok(SendProposalContentResponse { response: proposal_status_from(err)? }),
        }
    }

    async fn handle_finish_proposal_request(
        &mut self,
        proposal_id: ProposalId,
    ) -> BatcherResult<SendProposalContentResponse> {
        debug!("Send proposal content done for {}", proposal_id);

        self.close_input_transaction_stream(proposal_id)?;
        if self.is_active(proposal_id).await {
            self.proposal_manager.await_active_proposal().await;
        }

        let proposal_result =
            self.get_completed_proposal_result(proposal_id).await.expect("Proposal should exist.");
        let proposal_status = match proposal_result {
            Ok(commitment) => ProposalStatus::Finished(commitment),
            Err(err) => proposal_status_from(err)?,
        };
        Ok(SendProposalContentResponse { response: proposal_status })
    }

    async fn handle_abort_proposal_request(
        &mut self,
        proposal_id: ProposalId,
    ) -> BatcherResult<SendProposalContentResponse> {
        self.proposal_manager.abort_proposal(proposal_id).await;
        self.close_input_transaction_stream(proposal_id)?;
        Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
    }

    fn close_input_transaction_stream(&mut self, proposal_id: ProposalId) -> BatcherResult<()> {
        self.validate_tx_streams
            .remove(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?;
        Ok(())
    }

    fn get_height_from_storage(&mut self) -> BatcherResult<BlockNumber> {
        self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })
    }

    #[instrument(skip(self), err)]
    pub async fn get_height(&mut self) -> BatcherResult<GetHeightResponse> {
        let height = self.get_height_from_storage()?;
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
        let commitment = self
            .get_completed_proposal_result(proposal_id)
            .await
            .expect("Proposal should exist.")
            .map_err(|_| BatcherError::InternalError)?;

        Ok(GetProposalContentResponse { content: GetProposalContent::Finished(commitment) })
    }

    #[instrument(skip(self), err)]
    pub async fn add_sync_block(&mut self, sync_block: SyncBlock) -> BatcherResult<()> {
        if let Some(height) = self.active_height {
            info!("Aborting all work on height {} due to state sync.", height);
            self.abort_active_height().await;
            self.active_height = None;
        }

        let SyncBlock { state_diff, transaction_hashes } = sync_block;
        let address_to_nonce = state_diff.nonces.iter().map(|(k, v)| (*k, *v)).collect();
        let tx_hashes = transaction_hashes.into_iter().collect();

        // TODO(Arni): Assert the input `sync_block` corresponds to this `height`.
        self.commit_proposal_and_block(state_diff, address_to_nonce, tx_hashes).await
    }

    // TODO(dvir): return `BlockExecutionArtifacts`
    #[instrument(skip(self), err)]
    pub async fn decision_reached(&mut self, input: DecisionReachedInput) -> BatcherResult<()> {
        let proposal_id = input.proposal_id;
        let proposal_output = self
            .proposal_manager
            .take_proposal_result(proposal_id)
            .await
            .ok_or(BatcherError::ExecutedProposalNotFound { proposal_id })?
            .map_err(|_| BatcherError::InternalError)?;
        let ProposalOutput { state_diff, nonces: address_to_nonce, tx_hashes, .. } =
            proposal_output;

        self.commit_proposal_and_block(state_diff, address_to_nonce, tx_hashes).await
    }

    async fn commit_proposal_and_block(
        &mut self,
        state_diff: ThinStateDiff,
        address_to_nonce: HashMap<ContractAddress, Nonce>,
        tx_hashes: HashSet<TransactionHash>,
    ) -> BatcherResult<()> {
        // TODO: Keep the height from start_height or get it from the input.
        let height = self.get_height_from_storage()?;
        info!("Committing block at height {} and notifying mempool of the block.", height);
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

    async fn is_active(&self, proposal_id: ProposalId) -> bool {
        self.proposal_manager.get_active_proposal().await == Some(proposal_id)
    }

    // Returns a completed proposal result, either its commitment or an error if the proposal
    // failed. If the proposal doesn't exist, or it's still active, returns None.
    async fn get_completed_proposal_result(
        &self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<ProposalCommitment>> {
        let completed_proposals = self.proposal_manager.get_completed_proposals().await;
        let guard = completed_proposals.lock().await;
        let proposal_result = guard.get(&proposal_id);

        match proposal_result {
            Some(Ok(output)) => Some(Ok(output.commitment)),
            Some(Err(e)) => Some(Err(e.clone())),
            None => None,
        }
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

impl ComponentStarter for Batcher {}
