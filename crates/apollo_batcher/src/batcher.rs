use std::collections::HashMap;
use std::sync::Arc;

use apollo_batcher_types::batcher_types::{
    BatcherResult,
    CentralObjects,
    DecisionReachedInput,
    DecisionReachedResponse,
    GetHeightResponse,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    RevertBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use apollo_batcher_types::errors::BatcherError;
use apollo_class_manager_types::transaction_converter::TransactionConverter;
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_l1_provider_types::errors::{L1ProviderClientError, L1ProviderError};
use apollo_l1_provider_types::{SessionState, SharedL1ProviderClient};
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::CommitBlockArgs;
use apollo_reverts::revert_block;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::state::{StateStorageReader, StateStorageWriter};
use async_trait::async_trait;
use blockifier::concurrency::worker_pool::WorkerPool;
use blockifier::state::contract_class_manager::ContractClassManager;
use futures::FutureExt;
use indexmap::IndexSet;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::{BlockHeaderWithoutHash, BlockNumber};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, trace, Instrument};

use crate::block_builder::{
    BlockBuilderError,
    BlockBuilderExecutionParams,
    BlockBuilderFactory,
    BlockBuilderFactoryTrait,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    BlockMetadata,
};
use crate::cende_client_types::CendeBlockMetadata;
use crate::config::BatcherConfig;
use crate::metrics::{
    register_metrics,
    ProposalMetricsHandle,
    BATCHED_TRANSACTIONS,
    LAST_BATCHED_BLOCK,
    LAST_PROPOSED_BLOCK,
    LAST_SYNCED_BLOCK,
    REJECTED_TRANSACTIONS,
    REVERTED_BLOCKS,
    STORAGE_HEIGHT,
    SYNCED_TRANSACTIONS,
};
use crate::pre_confirmed_block_writer::{
    PreconfirmedBlockWriterFactory,
    PreconfirmedBlockWriterFactoryTrait,
    PreconfirmedBlockWriterTrait,
};
use crate::pre_confirmed_cende_client::PreconfirmedCendeClientTrait;
use crate::transaction_provider::{ProposeTransactionProvider, ValidateTransactionProvider};
use crate::utils::{
    deadline_as_instant,
    proposal_status_from,
    verify_block_input,
    ProposalResult,
    ProposalTask,
};

type OutputStreamReceiver = tokio::sync::mpsc::UnboundedReceiver<InternalConsensusTransaction>;
type InputStreamSender = tokio::sync::mpsc::Sender<InternalConsensusTransaction>;

pub struct Batcher {
    pub config: BatcherConfig,
    pub storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    pub storage_writer: Box<dyn BatcherStorageWriterTrait>,
    pub l1_provider_client: SharedL1ProviderClient,
    pub mempool_client: SharedMempoolClient,
    pub transaction_converter: TransactionConverter,

    /// Used to create block builders.
    /// Using the factory pattern to allow for easier testing.
    block_builder_factory: Box<dyn BlockBuilderFactoryTrait>,

    /// Used to create pre-confirmed block writers.
    pre_confirmed_block_writer_factory: Box<dyn PreconfirmedBlockWriterFactoryTrait>,

    /// The height that the batcher is currently working on.
    /// All proposals are considered to be at this height.
    active_height: Option<BlockNumber>,

    /// The block proposal that is currently being built, if any.
    /// At any given time, there can be only one proposal being actively executed (either proposed
    /// or validated).
    active_proposal: Arc<Mutex<Option<ProposalId>>>,
    active_proposal_task: Option<ProposalTask>,

    /// Holds all the proposals that completed execution in the current height.
    executed_proposals: Arc<Mutex<HashMap<ProposalId, ProposalResult<BlockExecutionArtifacts>>>>,

    /// The propose blocks transaction streams, used to stream out the proposal transactions.
    /// Each stream is kept until all the transactions are streamed out, or a new height is
    /// started.
    propose_tx_streams: HashMap<ProposalId, OutputStreamReceiver>,

    /// The validate blocks transaction streams, used to stream in the transactions to validate.
    /// Each stream is kept until SendProposalContent::Finish/Abort is received, or a new height is
    /// started.
    validate_tx_streams: HashMap<ProposalId, InputStreamSender>,
}

impl Batcher {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: BatcherConfig,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
        storage_writer: Box<dyn BatcherStorageWriterTrait>,
        l1_provider_client: SharedL1ProviderClient,
        mempool_client: SharedMempoolClient,
        transaction_converter: TransactionConverter,
        block_builder_factory: Box<dyn BlockBuilderFactoryTrait>,
        pre_confirmed_block_writer_factory: Box<dyn PreconfirmedBlockWriterFactoryTrait>,
    ) -> Self {
        Self {
            config,
            storage_reader,
            storage_writer,
            l1_provider_client,
            mempool_client,
            transaction_converter,
            block_builder_factory,
            pre_confirmed_block_writer_factory,
            active_height: None,
            active_proposal: Arc::new(Mutex::new(None)),
            active_proposal_task: None,
            executed_proposals: Arc::new(Mutex::new(HashMap::new())),
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
        if storage_height != input.height {
            return Err(BatcherError::StorageHeightMarkerMismatch {
                marker_height: storage_height,
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
        let block_number = propose_block_input.block_info.block_number;
        let proposal_metrics_handle = ProposalMetricsHandle::new();
        let active_height = self.active_height.ok_or(BatcherError::NoActiveHeight)?;
        verify_block_input(
            active_height,
            block_number,
            propose_block_input.retrospective_block_hash,
        )?;

        // TODO(yair): extract function for the following calls, use join_all.
        self.mempool_client.commit_block(CommitBlockArgs::default()).await.map_err(|err| {
            error!(
                "Mempool is not ready to start proposal {}: {}.",
                propose_block_input.proposal_id, err
            );
            BatcherError::NotReady
        })?;
        self.mempool_client
            .update_gas_price(
                propose_block_input.block_info.gas_prices.strk_gas_prices.l2_gas_price.get(),
            )
            .await
            .map_err(|err| {
                error!("Failed to update gas price in mempool: {}", err);
                BatcherError::InternalError
            })?;
        self.l1_provider_client
            .start_block(SessionState::Propose, propose_block_input.block_info.block_number)
            .await
            .map_err(|err| {
                error!(
                    "L1 provider is not ready to start proposing block {}: {}. ",
                    propose_block_input.block_info.block_number, err
                );
                BatcherError::NotReady
            })?;

        let tx_provider = ProposeTransactionProvider::new(
            self.mempool_client.clone(),
            self.l1_provider_client.clone(),
            self.config.max_l1_handler_txs_per_block_proposal,
            propose_block_input.block_info.block_number,
        );

        // A channel to receive the transactions included in the proposed block.
        let (output_tx_sender, output_tx_receiver) = tokio::sync::mpsc::unbounded_channel();

        let cende_block_metadata = CendeBlockMetadata::new(propose_block_input.block_info.clone());
        let (pre_confirmed_block_writer, candidate_tx_sender, pre_confirmed_tx_sender) =
            self.pre_confirmed_block_writer_factory.create(
                propose_block_input.block_info.block_number,
                propose_block_input.proposal_round,
                cende_block_metadata,
            );

        let (block_builder, abort_signal_sender) = self
            .block_builder_factory
            .create_block_builder(
                BlockMetadata {
                    block_info: propose_block_input.block_info,
                    retrospective_block_hash: propose_block_input.retrospective_block_hash,
                },
                BlockBuilderExecutionParams {
                    deadline: deadline_as_instant(propose_block_input.deadline)?,
                    is_validator: false,
                },
                Box::new(tx_provider),
                Some(output_tx_sender),
                Some(candidate_tx_sender),
                Some(pre_confirmed_tx_sender),
                tokio::runtime::Handle::current(),
            )
            .map_err(|err| {
                error!("Failed to get block builder: {}", err);
                BatcherError::InternalError
            })?;

        self.spawn_proposal(
            propose_block_input.proposal_id,
            block_builder,
            abort_signal_sender,
            None,
            Some(pre_confirmed_block_writer),
            proposal_metrics_handle,
        )
        .await?;

        let proposal_already_exists =
            self.propose_tx_streams.insert(propose_block_input.proposal_id, output_tx_receiver);
        assert!(
            proposal_already_exists.is_none(),
            "Proposal {} already exists. This should have been checked when spawning the proposal.",
            propose_block_input.proposal_id
        );
        LAST_PROPOSED_BLOCK.set_lossy(block_number.0);
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn validate_block(
        &mut self,
        validate_block_input: ValidateBlockInput,
    ) -> BatcherResult<()> {
        let proposal_metrics_handle = ProposalMetricsHandle::new();
        let active_height = self.active_height.ok_or(BatcherError::NoActiveHeight)?;
        verify_block_input(
            active_height,
            validate_block_input.block_info.block_number,
            validate_block_input.retrospective_block_hash,
        )?;

        self.l1_provider_client
            .start_block(SessionState::Validate, validate_block_input.block_info.block_number)
            .await
            .map_err(|err| {
                error!(
                    "L1 provider is not ready to start validating block {}: {}. ",
                    validate_block_input.block_info.block_number, err
                );
                BatcherError::NotReady
            })?;

        // A channel to send the transactions to include in the block being validated.
        let (input_tx_sender, input_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.input_stream_content_buffer_size);
        let (final_n_executed_txs_sender, final_n_executed_txs_receiver) =
            tokio::sync::oneshot::channel();

        let tx_provider = ValidateTransactionProvider::new(
            input_tx_receiver,
            final_n_executed_txs_receiver,
            self.l1_provider_client.clone(),
            validate_block_input.block_info.block_number,
        );

        let (block_builder, abort_signal_sender) = self
            .block_builder_factory
            .create_block_builder(
                BlockMetadata {
                    block_info: validate_block_input.block_info,
                    retrospective_block_hash: validate_block_input.retrospective_block_hash,
                },
                BlockBuilderExecutionParams {
                    deadline: deadline_as_instant(validate_block_input.deadline)?,
                    is_validator: true,
                },
                Box::new(tx_provider),
                None,
                None,
                None,
                tokio::runtime::Handle::current(),
            )
            .map_err(|err| {
                error!("Failed to get block builder: {}", err);
                BatcherError::InternalError
            })?;

        self.spawn_proposal(
            validate_block_input.proposal_id,
            block_builder,
            abort_signal_sender,
            Some(final_n_executed_txs_sender),
            None,
            proposal_metrics_handle,
        )
        .await?;

        let validation_already_exists =
            self.validate_tx_streams.insert(validate_block_input.proposal_id, input_tx_sender);
        assert!(
            validation_already_exists.is_none(),
            "Proposal {} already exists. This should have been checked when spawning the proposal.",
            validate_block_input.proposal_id
        );

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
            SendProposalContent::Finish(final_n_executed_txs) => {
                self.handle_finish_proposal_request(proposal_id, final_n_executed_txs).await
            }
            SendProposalContent::Abort => self.handle_abort_proposal_request(proposal_id).await,
        }
    }

    /// Clear all the proposals from the previous height.
    async fn abort_active_height(&mut self) {
        self.abort_active_proposal().await;
        self.executed_proposals.lock().await.clear();
        self.propose_tx_streams.clear();
        self.validate_tx_streams.clear();
        self.active_height = None;
    }

    async fn handle_send_txs_request(
        &mut self,
        proposal_id: ProposalId,
        txs: Vec<InternalConsensusTransaction>,
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
            Ok(_) => panic!("Proposal finished validation before all transactions were sent."),
            Err(err) => Ok(SendProposalContentResponse { response: proposal_status_from(err)? }),
        }
    }

    async fn handle_finish_proposal_request(
        &mut self,
        proposal_id: ProposalId,
        final_n_executed_txs: usize,
    ) -> BatcherResult<SendProposalContentResponse> {
        debug!("Send proposal content done for {}", proposal_id);

        self.validate_tx_streams.remove(&proposal_id).expect("validate tx stream should exist.");
        if self.is_active(proposal_id).await {
            self.await_active_proposal(final_n_executed_txs).await?;
        }

        let proposal_result =
            self.get_completed_proposal_result(proposal_id).await.expect("Proposal should exist.");
        let proposal_status = match proposal_result {
            Ok((commitment, _)) => ProposalStatus::Finished(commitment),
            Err(err) => proposal_status_from(err)?,
        };
        Ok(SendProposalContentResponse { response: proposal_status })
    }

    async fn handle_abort_proposal_request(
        &mut self,
        proposal_id: ProposalId,
    ) -> BatcherResult<SendProposalContentResponse> {
        if self.is_active(proposal_id).await {
            self.abort_active_proposal().await;

            let proposal_already_exists = self
                .executed_proposals
                .lock()
                .await
                .insert(proposal_id, Err(Arc::new(BlockBuilderError::Aborted)));
            assert!(proposal_already_exists.is_none(), "Duplicate proposal: {proposal_id}.");
        }
        self.validate_tx_streams.remove(&proposal_id);
        Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
    }

    fn get_height_from_storage(&self) -> BatcherResult<BlockNumber> {
        self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })
    }

    #[instrument(skip(self), err)]
    pub async fn get_height(&self) -> BatcherResult<GetHeightResponse> {
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
        self.propose_tx_streams.remove(&proposal_id);
        let (commitment, final_n_executed_txs) = self
            .get_completed_proposal_result(proposal_id)
            .await
            .expect("Proposal should exist.")
            .map_err(|err| {
                error!("Failed to get commitment: {}", err);
                BatcherError::InternalError
            })?;

        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished { id: commitment, final_n_executed_txs },
        })
    }

    #[instrument(skip(self, sync_block), err)]
    pub async fn add_sync_block(&mut self, sync_block: SyncBlock) -> BatcherResult<()> {
        trace!("Received sync block: {:?}", sync_block);
        // TODO(AlonH): Use additional data from the sync block.
        let SyncBlock {
            state_diff,
            account_transaction_hashes,
            l1_transaction_hashes,
            block_header_without_hash: BlockHeaderWithoutHash { block_number, .. },
        } = sync_block;

        let height = self.get_height_from_storage()?;
        if height != block_number {
            return Err(BatcherError::StorageHeightMarkerMismatch {
                marker_height: height,
                requested_height: block_number,
            });
        }

        if let Some(height) = self.active_height {
            info!("Aborting all work on height {} due to state sync.", height);
            self.abort_active_height().await;
        }

        let address_to_nonce = state_diff.nonces.iter().map(|(k, v)| (*k, *v)).collect();
        self.commit_proposal_and_block(
            height,
            state_diff,
            address_to_nonce,
            l1_transaction_hashes.iter().copied().collect(),
            Default::default(),
        )
        .await?;
        LAST_SYNCED_BLOCK.set_lossy(block_number.0);
        SYNCED_TRANSACTIONS.increment(
            (account_transaction_hashes.len() + l1_transaction_hashes.len()).try_into().unwrap(),
        );
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn decision_reached(
        &mut self,
        input: DecisionReachedInput,
    ) -> BatcherResult<DecisionReachedResponse> {
        let height = self.active_height.ok_or(BatcherError::NoActiveHeight)?;

        let proposal_id = input.proposal_id;
        let proposal_result = self.executed_proposals.lock().await.remove(&proposal_id);
        let block_execution_artifacts = proposal_result
            .ok_or(BatcherError::ExecutedProposalNotFound { proposal_id })?
            .map_err(|err| {
                error!("Failed to get block execution artifacts: {}", err);
                BatcherError::InternalError
            })?;
        let state_diff = block_execution_artifacts.thin_state_diff();
        let n_txs = u64::try_from(block_execution_artifacts.tx_hashes().len())
            .expect("Number of transactions should fit in u64");
        let n_rejected_txs =
            u64::try_from(block_execution_artifacts.execution_data.rejected_tx_hashes.len())
                .expect("Number of rejected transactions should fit in u64");
        self.commit_proposal_and_block(
            height,
            state_diff.clone(),
            block_execution_artifacts.address_to_nonce(),
            block_execution_artifacts.execution_data.consumed_l1_handler_tx_hashes,
            block_execution_artifacts.execution_data.rejected_tx_hashes,
        )
        .await?;
        let execution_infos = block_execution_artifacts.execution_data.execution_infos;

        LAST_BATCHED_BLOCK.set_lossy(height.0);
        BATCHED_TRANSACTIONS.increment(n_txs);
        REJECTED_TRANSACTIONS.increment(n_rejected_txs);

        Ok(DecisionReachedResponse {
            state_diff,
            l2_gas_used: block_execution_artifacts.l2_gas_used,
            central_objects: CentralObjects {
                execution_infos,
                bouncer_weights: block_execution_artifacts.bouncer_weights,
                compressed_state_diff: block_execution_artifacts.compressed_state_diff,
                casm_hash_computation_data_sierra_gas: block_execution_artifacts
                    .casm_hash_computation_data_sierra_gas,
                casm_hash_computation_data_proving_gas: block_execution_artifacts
                    .casm_hash_computation_data_proving_gas,
            },
        })
    }

    async fn commit_proposal_and_block(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        address_to_nonce: HashMap<ContractAddress, Nonce>,
        consumed_l1_handler_tx_hashes: IndexSet<TransactionHash>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
    ) -> BatcherResult<()> {
        info!(
            "Committing block at height {} and notifying mempool & L1 event provider of the block.",
            height
        );
        trace!("Rejected transactions: {:#?}, State diff: {:#?}.", rejected_tx_hashes, state_diff);

        // Commit the proposal to the storage.
        self.storage_writer.commit_proposal(height, state_diff).map_err(|err| {
            error!("Failed to commit proposal to storage: {}", err);
            BatcherError::InternalError
        })?;

        // Notify the L1 provider of the new block.
        let rejected_l1_handler_tx_hashes = rejected_tx_hashes
            .iter()
            .copied()
            .filter(|tx_hash| consumed_l1_handler_tx_hashes.contains(tx_hash))
            .collect();

        let l1_provider_result = self
            .l1_provider_client
            .commit_block(consumed_l1_handler_tx_hashes, rejected_l1_handler_tx_hashes, height)
            .await;

        // Return error if the commit to the L1 provider failed.
        if let Err(err) = l1_provider_result {
            match err {
                L1ProviderClientError::L1ProviderError(L1ProviderError::UnexpectedHeight {
                    expected_height,
                    got,
                }) => {
                    error!(
                        "Unexpected height while committing block in L1 provider: expected={:?}, \
                         got={:?}",
                        expected_height, got
                    );
                }
                other_err => {
                    error!(
                        "Unexpected error while committing block in L1 provider: {:?}",
                        other_err
                    );
                }
            }
            // Rollback the state diff in the storage.
            self.storage_writer.revert_block(height);
            return Err(BatcherError::InternalError);
        }

        // Notify the mempool of the new block.
        let mempool_result = self
            .mempool_client
            .commit_block(CommitBlockArgs { address_to_nonce, rejected_tx_hashes })
            .await;

        if let Err(mempool_err) = mempool_result {
            error!("Failed to commit block to mempool: {}", mempool_err);
            // TODO(AlonH): Should we rollback the state diff and return an error?
        };

        STORAGE_HEIGHT.increment(1);
        Ok(())
    }

    async fn is_active(&self, proposal_id: ProposalId) -> bool {
        *self.active_proposal.lock().await == Some(proposal_id)
    }

    // Sets a new active proposal task.
    // Fails if there is another proposal being currently generated, or a proposal with the same ID
    // already exists.
    async fn set_active_proposal(&mut self, proposal_id: ProposalId) -> BatcherResult<()> {
        if self.executed_proposals.lock().await.contains_key(&proposal_id) {
            return Err(BatcherError::ProposalAlreadyExists { proposal_id });
        }

        let mut active_proposal = self.active_proposal.lock().await;
        if let Some(active_proposal_id) = *active_proposal {
            return Err(BatcherError::AnotherProposalInProgress {
                active_proposal_id,
                new_proposal_id: proposal_id,
            });
        }

        debug!("Set proposal {} as the one being generated.", proposal_id);
        *active_proposal = Some(proposal_id);
        Ok(())
    }

    // Starts a new block proposal generation task for the given proposal_id.
    // Uses the given block_builder to generate the proposal.
    async fn spawn_proposal(
        &mut self,
        proposal_id: ProposalId,
        mut block_builder: Box<dyn BlockBuilderTrait>,
        abort_signal_sender: tokio::sync::oneshot::Sender<()>,
        final_n_executed_txs_sender: Option<tokio::sync::oneshot::Sender<usize>>,
        pre_confirmed_block_writer: Option<Box<dyn PreconfirmedBlockWriterTrait>>,
        mut proposal_metrics_handle: ProposalMetricsHandle,
    ) -> BatcherResult<()> {
        self.set_active_proposal(proposal_id).await?;
        info!("Starting generation of a new proposal with id {}.", proposal_id);

        let active_proposal = self.active_proposal.clone();
        let executed_proposals = self.executed_proposals.clone();

        let execution_join_handle = tokio::spawn(
            async move {
                let result = match block_builder.build_block().await {
                    Ok(artifacts) => {
                        proposal_metrics_handle.set_succeeded();
                        Ok(artifacts)
                    }
                    Err(BlockBuilderError::Aborted) => {
                        proposal_metrics_handle.set_aborted();
                        Err(BlockBuilderError::Aborted)
                    }
                    Err(e) => Err(e),
                }
                .map_err(Arc::new);

                // The proposal is done, clear the active proposal.
                // Keep the proposal result only if it is the same as the active proposal.
                // The active proposal might have changed if this proposal was aborted.
                let mut active_proposal = active_proposal.lock().await;
                if *active_proposal == Some(proposal_id) {
                    active_proposal.take();
                    let proposal_already_exists =
                        executed_proposals.lock().await.insert(proposal_id, result);
                    assert!(
                        proposal_already_exists.is_none(),
                        "Duplicate proposal: {proposal_id}."
                    );
                }
            }
            .in_current_span(),
        );

        let writer_join_handle =
            pre_confirmed_block_writer.map(|mut pre_confirmed_block_writer| {
                tokio::spawn(async move {
                    // TODO(noamsp): add error handling
                    pre_confirmed_block_writer.run().await.ok();
                })
            });

        self.active_proposal_task = Some(ProposalTask {
            abort_signal_sender,
            final_n_executed_txs_sender,
            execution_join_handle,
            writer_join_handle,
        });
        Ok(())
    }

    // Returns a completed proposal result, either its commitment and final_n_executed_txs or an
    // error if the proposal failed. If the proposal doesn't exist, or it's still active,
    // returns None.
    async fn get_completed_proposal_result(
        &self,
        proposal_id: ProposalId,
    ) -> Option<ProposalResult<(ProposalCommitment, usize)>> {
        let guard = self.executed_proposals.lock().await;
        let proposal_result = guard.get(&proposal_id);
        match proposal_result {
            Some(Ok(artifacts)) => {
                Some(Ok((artifacts.commitment(), artifacts.final_n_executed_txs)))
            }
            Some(Err(e)) => Some(Err(e.clone())),
            None => None,
        }
    }

    // Ends the current active proposal.
    // This call is non-blocking.
    async fn abort_active_proposal(&mut self) {
        self.active_proposal.lock().await.take();
        if let Some(proposal_task) = self.active_proposal_task.take() {
            proposal_task.abort_signal_sender.send(()).ok();
        }
    }

    pub async fn await_active_proposal(
        &mut self,
        final_n_executed_txs: usize,
    ) -> BatcherResult<()> {
        if let Some(ProposalTask {
            execution_join_handle,
            writer_join_handle,
            final_n_executed_txs_sender,
            ..
        }) = self.active_proposal_task.take()
        {
            if let Some(final_n_executed_txs_sender) = final_n_executed_txs_sender {
                final_n_executed_txs_sender.send(final_n_executed_txs).map_err(|err| {
                    error!(
                        "Failed to send final_n_executed_txs ({final_n_executed_txs}) to the tx \
                         provider: {}",
                        err
                    );
                    BatcherError::InternalError
                })?;
            }

            let writer_future = writer_join_handle
                .map(FutureExt::boxed)
                .unwrap_or_else(|| futures::future::ready(Ok(())).boxed());
            let _ = tokio::join!(execution_join_handle, writer_future);
        }

        Ok(())
    }

    #[instrument(skip(self), err)]
    // This function will panic if there is a storage failure to revert the block.
    pub async fn revert_block(&mut self, input: RevertBlockInput) -> BatcherResult<()> {
        info!("Reverting block at height {}.", input.height);
        let height = self.get_height_from_storage()?.prev().ok_or(
            BatcherError::StorageHeightMarkerMismatch {
                marker_height: BlockNumber(0),
                requested_height: input.height,
            },
        )?;

        if height != input.height {
            return Err(BatcherError::StorageHeightMarkerMismatch {
                marker_height: height.unchecked_next(),
                requested_height: input.height,
            });
        }

        if let Some(height) = self.active_height {
            info!("Aborting all work on height {} due to a revert request.", height);
            self.abort_active_height().await;
        }

        self.storage_writer.revert_block(height);
        STORAGE_HEIGHT.decrement(1);
        REVERTED_BLOCKS.increment(1);
        Ok(())
    }
}

pub fn create_batcher(
    config: BatcherConfig,
    mempool_client: SharedMempoolClient,
    l1_provider_client: SharedL1ProviderClient,
    class_manager_client: SharedClassManagerClient,
    pre_confirmed_cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
) -> Batcher {
    let (storage_reader, storage_writer) = apollo_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");

    let execute_config = &config.block_builder_config.execute_config;
    let worker_pool = Arc::new(WorkerPool::start(execute_config));
    let pre_confirmed_block_writer_factory = Box::new(PreconfirmedBlockWriterFactory {
        config: config.pre_confirmed_block_writer_config,
        cende_client: pre_confirmed_cende_client,
    });
    let block_builder_factory = Box::new(BlockBuilderFactory {
        block_builder_config: config.block_builder_config.clone(),
        storage_reader: storage_reader.clone(),
        contract_class_manager: ContractClassManager::start(
            config.contract_class_manager_config.clone(),
        ),
        class_manager_client: class_manager_client.clone(),
        worker_pool,
    });
    let storage_reader = Arc::new(storage_reader);
    let storage_writer = Box::new(storage_writer);
    let transaction_converter =
        TransactionConverter::new(class_manager_client, config.storage.db_config.chain_id.clone());

    Batcher::new(
        config,
        storage_reader,
        storage_writer,
        l1_provider_client,
        mempool_client,
        transaction_converter,
        block_builder_factory,
        pre_confirmed_block_writer_factory,
    )
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {
    /// Returns the next height that the batcher should work on.
    fn height(&self) -> apollo_storage::StorageResult<BlockNumber>;
}

impl BatcherStorageReaderTrait for apollo_storage::StorageReader {
    fn height(&self) -> apollo_storage::StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_state_marker()
    }
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageWriterTrait: Send + Sync {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
    ) -> apollo_storage::StorageResult<()>;

    fn revert_block(&mut self, height: BlockNumber);
}

impl BatcherStorageWriterTrait for apollo_storage::StorageWriter {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
    ) -> apollo_storage::StorageResult<()> {
        // TODO(AlonH): write casms.
        self.begin_rw_txn()?.append_state_diff(height, state_diff)?.commit()
    }

    // This function will panic if there is a storage failure to revert the block.
    fn revert_block(&mut self, height: BlockNumber) {
        revert_block(self, height);
    }
}

#[async_trait]
impl ComponentStarter for Batcher {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        let storage_height = self
            .storage_reader
            .height()
            .expect("Failed to get height from storage during batcher creation.");
        register_metrics(storage_height);
    }
}
