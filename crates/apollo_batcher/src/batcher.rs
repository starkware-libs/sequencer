use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

use apollo_batcher_config::config::{
    BatcherConfig,
    BatcherDynamicConfig,
    FirstBlockWithPartialBlockHash,
};
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
use apollo_committer_types::committer_types::RevertBlockResponse;
use apollo_committer_types::communication::SharedCommitterClient;
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_l1_provider_types::errors::{L1ProviderClientError, L1ProviderError};
use apollo_l1_provider_types::{SessionState, SharedL1ProviderClient};
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::CommitBlockArgs;
use apollo_reverts::revert_block;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::block_hash::{BlockHashStorageReader, BlockHashStorageWriter};
use apollo_storage::global_root::{GlobalRootStorageReader, GlobalRootStorageWriter};
use apollo_storage::global_root_marker::{
    GlobalRootMarkerStorageReader,
    GlobalRootMarkerStorageWriter,
};
use apollo_storage::metrics::BATCHER_STORAGE_OPEN_READ_TRANSACTIONS;
use apollo_storage::partial_block_hash::{
    PartialBlockHashComponentsStorageReader,
    PartialBlockHashComponentsStorageWriter,
};
use apollo_storage::state::{StateStorageReader, StateStorageWriter};
use apollo_storage::storage_reader_server::ServerConfig;
use apollo_storage::storage_reader_types::GenericStorageReaderServer;
use apollo_storage::{
    open_storage_with_metric_and_server,
    StorageError,
    StorageReader,
    StorageResult,
    StorageWriter,
};
use async_trait::async_trait;
use blockifier::concurrency::worker_pool::WorkerPool;
use blockifier::state::contract_class_manager::ContractClassManager;
use futures::FutureExt;
use indexmap::{IndexMap, IndexSet};
#[cfg(test)]
use mockall::automock;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, GlobalRoot, Nonce, StateDiffCommitment};
use starknet_api::state::{StateNumber, ThinStateDiff};
use starknet_api::transaction::TransactionHash;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
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
use crate::commitment_manager::commitment_manager_impl::{
    ApolloCommitmentManager,
    CommitmentManager,
};
use crate::commitment_manager::types::RevertTaskOutput;
use crate::metrics::{
    register_metrics,
    ProposalMetricsHandle,
    BATCHED_TRANSACTIONS,
    BATCHER_L1_PROVIDER_ERRORS,
    BUILDING_HEIGHT,
    GLOBAL_ROOT_HEIGHT,
    L2_GAS_IN_LAST_BLOCK,
    LAST_BATCHED_BLOCK_HEIGHT,
    LAST_PROPOSED_BLOCK_HEIGHT,
    LAST_SYNCED_BLOCK_HEIGHT,
    NUM_TRANSACTION_IN_BLOCK,
    PROVING_GAS_IN_LAST_BLOCK,
    REJECTED_TRANSACTIONS,
    REVERTED_BLOCKS,
    REVERTED_TRANSACTIONS,
    SIERRA_GAS_IN_LAST_BLOCK,
    SYNCED_TRANSACTIONS,
};
use crate::pre_confirmed_block_writer::{
    PreconfirmedBlockWriterFactory,
    PreconfirmedBlockWriterFactoryTrait,
    PreconfirmedBlockWriterTrait,
};
use crate::pre_confirmed_cende_client::PreconfirmedCendeClientTrait;
use crate::transaction_provider::{
    ProposeTransactionProvider,
    TxProviderPhase,
    ValidateTransactionProvider,
};
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
    pub storage_reader: Arc<dyn BatcherStorageReader>,
    pub storage_writer: Box<dyn BatcherStorageWriter>,
    pub committer_client: SharedCommitterClient,
    pub l1_provider_client: SharedL1ProviderClient,
    pub mempool_client: SharedMempoolClient,
    pub transaction_converter: TransactionConverter,
    pub config_manager_client: SharedConfigManagerClient,

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

    /// Number of proposals made since coming online.
    proposals_counter: u64,

    /// The proposal commitment of the previous height.
    /// This is returned by the decision_reached function.
    prev_proposal_commitment: Option<(BlockNumber, ProposalCommitment)>,

    // TODO(Yoav): Use `apollo_proc_macros::make_visibility` once it supports fields.
    #[cfg(test)]
    pub(crate) commitment_manager: ApolloCommitmentManager,
    #[cfg(not(test))]
    commitment_manager: ApolloCommitmentManager,

    // Kept alive to maintain the server running.
    #[allow(dead_code)]
    storage_reader_server_handle: Option<AbortHandle>,
}

impl Batcher {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: BatcherConfig,
        storage_reader: Arc<dyn BatcherStorageReader>,
        storage_writer: Box<dyn BatcherStorageWriter>,
        committer_client: SharedCommitterClient,
        l1_provider_client: SharedL1ProviderClient,
        mempool_client: SharedMempoolClient,
        transaction_converter: TransactionConverter,
        config_manager_client: SharedConfigManagerClient,
        block_builder_factory: Box<dyn BlockBuilderFactoryTrait>,
        pre_confirmed_block_writer_factory: Box<dyn PreconfirmedBlockWriterFactoryTrait>,
        commitment_manager: ApolloCommitmentManager,
        storage_reader_server_handle: Option<AbortHandle>,
    ) -> Self {
        Self {
            config,
            storage_reader,
            storage_writer,
            committer_client,
            l1_provider_client,
            mempool_client,
            transaction_converter,
            config_manager_client,
            block_builder_factory,
            pre_confirmed_block_writer_factory,
            active_height: None,
            active_proposal: Arc::new(Mutex::new(None)),
            active_proposal_task: None,
            executed_proposals: Arc::new(Mutex::new(HashMap::new())),
            propose_tx_streams: HashMap::new(),
            validate_tx_streams: HashMap::new(),
            // Allow the first few proposals to be without L1 txs while system starts up.
            proposals_counter: 1,
            prev_proposal_commitment: None,
            commitment_manager,
            storage_reader_server_handle,
        }
    }

    pub(crate) fn update_dynamic_config(&mut self, dynamic_config: BatcherDynamicConfig) {
        self.config.dynamic_config = dynamic_config;
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

    // TODO(Rotem): Once the fallback option to state sync is removed - remove
    // `retrospective_block_hash` from the input and get it from storage instead.
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
        info!(
            "Notifying the mempool we start to work on block {}, round {}.",
            block_number, propose_block_input.proposal_round
        );
        self.mempool_client.commit_block(CommitBlockArgs::default()).await.map_err(|err| {
            error!(
                "Mempool is not ready to start proposal {}: {}.",
                propose_block_input.proposal_id, err
            );
            BatcherError::NotReady
        })?;
        info!(
            "Updating gas price for block {}, round {} in Mempool client",
            block_number, propose_block_input.proposal_round
        );
        self.mempool_client
            .update_gas_price(
                propose_block_input.block_info.gas_prices.strk_gas_prices.l2_gas_price.get(),
            )
            .await
            .map_err(|err| {
                error!("Failed to update gas price in mempool: {}", err);
                BatcherError::InternalError
            })?;
        // Ignore errors. If start_block fails, then subsequent calls to l1 provider will fail on
        // out of session and l1 provider will restart and bootstrap again.
        let _ = self
            .l1_provider_client
            .start_block(SessionState::Propose, propose_block_input.block_info.block_number)
            .await
            .inspect_err(|err| {
                error!(
                    "L1 provider is not ready to start proposing block {}: {}. ",
                    propose_block_input.block_info.block_number, err
                );
                BATCHER_L1_PROVIDER_ERRORS.increment(1);
            });

        let start_phase = if self
            .proposals_counter
            .is_multiple_of(self.config.static_config.propose_l1_txs_every)
        {
            TxProviderPhase::L1
        } else {
            TxProviderPhase::Mempool
        };
        let tx_provider = ProposeTransactionProvider::new(
            self.mempool_client.clone(),
            self.l1_provider_client.clone(),
            self.config.static_config.max_l1_handler_txs_per_block_proposal,
            propose_block_input.block_info.block_number,
            start_phase,
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
                    proposer_idle_detection_delay: self
                        .config
                        .static_config
                        .block_builder_config
                        .proposer_idle_detection_delay_millis,
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
        LAST_PROPOSED_BLOCK_HEIGHT.set_lossy(block_number.0);
        self.proposals_counter += 1;
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

        // Ignore errors. If start_block fails, then subsequent calls to l1 provider will fail on
        // out of session and l1 provider will restart and bootstrap again.
        let _ = self
            .l1_provider_client
            .start_block(SessionState::Validate, validate_block_input.block_info.block_number)
            .await
            .inspect_err(|err| {
                error!(
                    "L1 provider is not ready to start validating block {}: {}. ",
                    validate_block_input.block_info.block_number, err
                );
                BATCHER_L1_PROVIDER_ERRORS.increment(1);
            });

        // A channel to send the transactions to include in the block being validated.
        let (input_tx_sender, input_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.static_config.input_stream_content_buffer_size);
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
                    proposer_idle_detection_delay: self
                        .config
                        .static_config
                        .block_builder_config
                        .proposer_idle_detection_delay_millis,
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
    #[cfg_attr(any(test, feature = "testing"), apollo_proc_macros::make_visibility(pub))]
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
        info!(
            "BATCHER_FIN_VALIDATOR: Send proposal content done for {}. n_txs: {}",
            proposal_id, final_n_executed_txs
        );

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
        self.storage_reader.state_diff_height().map_err(|err| {
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
        let n_executed_txs = tx_stream
            .recv_many(&mut txs, self.config.static_config.outstream_content_buffer_size)
            .await;

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
        info!(
            "BATCHER_FIN_PROPOSER: Finished building proposal {proposal_id} with \
             {final_n_executed_txs} transactions."
        );
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
            block_header_without_hash,
            block_header_commitments,
        } = sync_block;
        let block_number = block_header_without_hash.block_number;

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

        let storage_commitment_block_hash = if block_header_without_hash
            .starknet_version
            .has_partial_block_hash_components()
        {
            self.maybe_handle_first_block_with_partial_block_hash(
                block_header_without_hash.parent_hash,
                block_number,
            )
            .map_err(|err| {
                error!("Error handling block number {block_number} with partial block hash: {err}");
                BatcherError::InternalError
            })?;
            match block_header_commitments {
                Some(header_commitments) => {
                    StorageCommitmentBlockHash::Partial(PartialBlockHashComponents {
                        header_commitments,
                        block_number,
                        l1_gas_price: block_header_without_hash.l1_gas_price,
                        l1_data_gas_price: block_header_without_hash.l1_data_gas_price,
                        l2_gas_price: block_header_without_hash.l2_gas_price,
                        sequencer: block_header_without_hash.sequencer,
                        timestamp: block_header_without_hash.timestamp,
                        starknet_version: block_header_without_hash.starknet_version,
                    })
                }
                None => return Err(BatcherError::MissingHeaderCommitments { block_number }),
            }
        } else {
            let first_block_with_partial_block_hash_number = self
                .config
                .static_config
                .first_block_with_partial_block_hash
                .as_ref()
                .expect(
                    "Since an old block was learned via sync, first block with partial block hash \
                     components should be configured.",
                )
                .block_number;
            assert!(
                height < first_block_with_partial_block_hash_number,
                "Height {height} is at least the first block configured to include a partial hash \
                 ({first_block_with_partial_block_hash_number}) but does not include one.",
            );
            StorageCommitmentBlockHash::ParentHash(block_header_without_hash.parent_hash)
        };

        let optional_state_diff_commitment = match &storage_commitment_block_hash {
            StorageCommitmentBlockHash::ParentHash(_) => None,
            StorageCommitmentBlockHash::Partial(PartialBlockHashComponents {
                ref header_commitments,
                ..
            }) => Some(header_commitments.state_diff_commitment),
        };

        // Verify that the synced state diff commitment matches what would be calculated from the
        // provided state diff. This prevents committing incorrect state diffs that could lead to
        // ProposalFinMismatch errors when building subsequent blocks.
        if let Some(synced_commitment) = optional_state_diff_commitment {
            let calculated_commitment = calculate_state_diff_hash(&state_diff);
            if synced_commitment != calculated_commitment {
                error!(
                    "Synced state diff commitment mismatch for block {block_number}. Synced: \
                     {synced_commitment:?}, Calculated: {calculated_commitment:?}"
                );
                return Err(BatcherError::InternalError);
            }
        }

        // Verify that if we already have a state diff for this block (from consensus or previous
        // sync), it matches the synced state diff. This prevents overwriting a
        // consensus-committed state diff with a different synced one, which would cause
        // ProposalFinMismatch errors.
        if let Ok(Some(existing_state_diff)) = self.storage_reader.get_state_diff(block_number) {
            let existing_commitment = calculate_state_diff_hash(&existing_state_diff);
            let synced_commitment = match optional_state_diff_commitment {
                Some(commitment) => commitment,
                None => calculate_state_diff_hash(&state_diff),
            };
            if existing_commitment != synced_commitment {
                error!(
                    "Cannot sync block {block_number}: existing state diff commitment \
                     ({existing_commitment:?}) does not match synced state diff commitment \
                     ({synced_commitment:?}). This would cause ProposalFinMismatch errors."
                );
                return Err(BatcherError::InternalError);
            }
            // State diffs match - log but continue with commit to ensure metrics and other state
            // are updated
            info!(
                "Block {block_number} already has matching state diff in storage, but continuing \
                 with sync commit for consistency."
            );
        }

        self.commit_proposal_and_block(
            height,
            state_diff.clone(),
            address_to_nonce,
            l1_transaction_hashes.iter().copied().collect(),
            Default::default(),
            storage_commitment_block_hash,
        )
        .await?;

        self.write_commitment_results_and_add_new_task(
            height,
            state_diff,
            optional_state_diff_commitment,
        )
        .await?;

        LAST_SYNCED_BLOCK_HEIGHT.set_lossy(block_number.0);
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
        let n_reverted_count = u64::try_from(
            block_execution_artifacts
                .execution_data
                .execution_infos_and_signatures
                .values()
                .filter(|(info, _)| info.revert_error.is_some())
                .count(),
        )
        .expect("Number of reverted transactions should fit in u64");
        let partial_block_hash_components =
            block_execution_artifacts.partial_block_hash_components();
        let state_diff_commitment =
            partial_block_hash_components.header_commitments.state_diff_commitment;
        let block_header_commitments = partial_block_hash_components.header_commitments.clone();
        let parent_proposal_commitment = self.get_parent_proposal_commitment(height)?;
        self.commit_proposal_and_block(
            height,
            state_diff.clone(),
            block_execution_artifacts.address_to_nonce(),
            block_execution_artifacts.execution_data.consumed_l1_handler_tx_hashes,
            block_execution_artifacts.execution_data.rejected_tx_hashes,
            StorageCommitmentBlockHash::Partial(partial_block_hash_components),
        )
        .await?;

        self.write_commitment_results_and_add_new_task(
            height,
            state_diff.clone(), // TODO(Nimrod): Remove the clone here.
            Some(state_diff_commitment),
        )
        .await?;

        let execution_infos = block_execution_artifacts
            .execution_data
            .execution_infos_and_signatures
            .into_iter()
            .map(|(tx_hash, (info, _))| (tx_hash, info))
            .collect();

        LAST_BATCHED_BLOCK_HEIGHT.set_lossy(height.0);
        BATCHED_TRANSACTIONS.increment(n_txs);
        REJECTED_TRANSACTIONS.increment(n_rejected_txs);
        REVERTED_TRANSACTIONS.increment(n_reverted_count);
        NUM_TRANSACTION_IN_BLOCK.record_lossy(n_txs);
        SIERRA_GAS_IN_LAST_BLOCK.set_lossy(block_execution_artifacts.bouncer_weights.sierra_gas.0);
        PROVING_GAS_IN_LAST_BLOCK
            .set_lossy(block_execution_artifacts.bouncer_weights.proving_gas.0);
        L2_GAS_IN_LAST_BLOCK.set_lossy(block_execution_artifacts.l2_gas_used.0);

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
                compiled_class_hashes_for_migration: block_execution_artifacts
                    .compiled_class_hashes_for_migration,
                parent_proposal_commitment,
            },
            block_header_commitments,
        })
    }

    async fn commit_proposal_and_block(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        address_to_nonce: HashMap<ContractAddress, Nonce>,
        consumed_l1_handler_tx_hashes: IndexSet<TransactionHash>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
        storage_commitment_block_hash: StorageCommitmentBlockHash,
    ) -> BatcherResult<()> {
        info!(
            "Committing block at height {} and notifying mempool & L1 event provider of the block.",
            height
        );
        trace!("Rejected transactions: {:#?}, State diff: {:#?}.", rejected_tx_hashes, state_diff);

        let state_diff_commitment = calculate_state_diff_hash(&state_diff);

        // Commit the proposal to the storage.
        self.storage_writer
            .commit_proposal(height, state_diff, storage_commitment_block_hash)
            .map_err(|err| {
                error!("Failed to commit proposal to storage: {}", err);
                BatcherError::InternalError
            })?;
        info!("Successfully committed proposal for block {} to storage.", height);
        self.prev_proposal_commitment =
            Some((height, ProposalCommitment { state_diff_commitment }));

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
            BATCHER_L1_PROVIDER_ERRORS.increment(1);
        }

        // Notify the mempool of the new block.
        let mempool_result = self
            .mempool_client
            .commit_block(CommitBlockArgs { address_to_nonce, rejected_tx_hashes })
            .await;

        if let Err(mempool_err) = mempool_result {
            // Recoverable error, mempool won't be updated with the new block.
            error!("Failed to commit block to mempool: {}", mempool_err);
        };

        BUILDING_HEIGHT.increment(1);
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

                    log_txs_execution_result(proposal_id, &result);

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

    /// If the optional configuration
    /// [`apollo_batcher_config::config::BatcherStaticConfig::first_block_with_partial_block_hash`]
    /// is set, and the given height is the first block with partial block hash components, we
    /// will set the parent hash of this block and verify the configured value.
    /// Assumption: we call this function only for new blocks.
    fn maybe_handle_first_block_with_partial_block_hash(
        &mut self,
        parent_block_hash_from_sync: BlockHash,
        height: BlockNumber,
    ) -> StorageResult<()> {
        let Some(FirstBlockWithPartialBlockHash { block_number, parent_block_hash, .. }) =
            self.config.static_config.first_block_with_partial_block_hash.as_ref()
        else {
            // No config is set, nothing to do.
            return Ok(());
        };
        assert!(
            height >= *block_number,
            "Block number {height} is a new block but is older than the configured first block \
             with partial block hash components {block_number}"
        );
        if height > *block_number {
            // Config is set but given height is not the first new block - nothing to do.
            return Ok(());
        }
        // Sanity check: verify that the parent block hash from sync matches the configured one.
        assert_eq!(
            *parent_block_hash, parent_block_hash_from_sync,
            "The parent block hash from sync ({parent_block_hash_from_sync:?}) does not match the \
             configured parent block hash ({parent_block_hash:?}) of the first new block"
        );

        info!(
            "The first block with partial block hash components ({block_number}) has been \
             reached. Subsequent blocks will include partial block hash components."
        );

        // Set the parent hash of the first block with partial block hash components.
        let parent_block_number = block_number
            .prev()
            .expect("First block with partial block hash should not be the genesis block.");
        self.storage_writer.set_block_hash(parent_block_number, *parent_block_hash)
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

        // Wait for the revert commitment to be completed before reverting the storage.
        self.revert_commitment(height).await;

        self.storage_writer.revert_block(height);
        BUILDING_HEIGHT.decrement(1);
        GLOBAL_ROOT_HEIGHT.decrement(1);
        REVERTED_BLOCKS.increment(1);
        Ok(())
    }

    // Returns the proposal commitment of the previous height.
    // NOTE: Assumes that the previous height was committed to the storage.
    fn get_parent_proposal_commitment(
        &mut self,
        height: BlockNumber,
    ) -> BatcherResult<Option<ProposalCommitment>> {
        let Some(prev_height) = height.prev() else {
            // This is the first block, so there is no parent proposal commitment.
            return Ok(None);
        };

        match self.prev_proposal_commitment {
            Some((h, commitment)) => {
                assert_eq!(h, prev_height, "Unexpected height of parent_proposal_commitment.");
                Ok(Some(commitment))
            }
            None => {
                // Parent proposal commitment is not cached. Compute it from the stored state diff.
                let state_diff = self
                    .storage_reader
                    .get_state_diff(prev_height)
                    .map_err(|err| {
                        error!(
                            "Failed to read state diff for previous height {prev_height}: {}",
                            err
                        );
                        BatcherError::InternalError
                    })?
                    .expect("Missing state diff for previous height.");

                Ok(Some(ProposalCommitment {
                    state_diff_commitment: calculate_state_diff_hash(&state_diff),
                }))
            }
        }
    }

    /// Reverts the commitment for the given height.
    /// Adds a revert task to the commitment manager channel and waits for the result.
    /// Writes commitment results to storage and handles the revert task result.
    async fn revert_commitment(&mut self, height: BlockNumber) {
        let reversed_state_diff = self
            .storage_reader
            .reversed_state_diff(height)
            .expect("Failed to get reversed state diff from storage.");
        self.commitment_manager
            .add_revert_task(
                height,
                reversed_state_diff,
                &self.config.static_config.first_block_with_partial_block_hash,
                self.storage_reader.clone(),
                &mut self.storage_writer,
            )
            .await
            .expect("Failed to add revert task to commitment manager.");
        let (commitment_results, revert_task_result) =
            self.commitment_manager.wait_for_revert_result().await;
        self.commitment_manager
            .write_commitment_results_to_storage(
                commitment_results,
                &self.config.static_config.first_block_with_partial_block_hash,
                self.storage_reader.clone(),
                &mut self.storage_writer,
            )
            .expect("Failed to write commitment results to storage.");

        info!("Revert task result: {revert_task_result:?}");
        self.validate_revert_task_result(revert_task_result, height).await;
        info!("Reverted commitment for height {height}.");
    }

    async fn validate_revert_task_result(
        &self,
        revert_task_output: RevertTaskOutput,
        request_height_to_revert: BlockNumber,
    ) {
        assert_eq!(
            revert_task_output.height, request_height_to_revert,
            "The task output height does not match the request height."
        );

        match revert_task_output.response {
            RevertBlockResponse::RevertedTo(global_root)
            | RevertBlockResponse::AlreadyReverted(global_root) => {
                // Verify the global root matches the stored global root.
                let new_latest_height = revert_task_output
                    .height
                    .prev()
                    .expect("Can't revert before the genesis block.");
                let stored_global_root = self
                    .storage_reader
                    .global_root(new_latest_height)
                    .expect("Failed to get global root from storage.")
                    .expect("Global root is not set for height {new_latest_height}.");
                assert_eq!(
                    global_root, stored_global_root,
                    "The given global root does not match the stored global root for height \
                     {new_latest_height}."
                );
            }
            RevertBlockResponse::Uncommitted => {}
        }
    }

    pub fn get_block_hash(&mut self, block_number: BlockNumber) -> BatcherResult<BlockHash> {
        self.get_commitment_results_and_write_to_storage()?;
        self.storage_reader
            .get_block_hash(block_number)
            .map_err(|err| {
                error!("Failed to get block hash from storage: {err}");
                BatcherError::InternalError
            })?
            .ok_or(BatcherError::BlockHashNotFound(block_number))
    }

    fn get_commitment_results_and_write_to_storage(&mut self) -> BatcherResult<()> {
        self.commitment_manager
            .get_commitment_results_and_write_to_storage(
                &self.config.static_config.first_block_with_partial_block_hash,
                self.storage_reader.clone(),
                &mut self.storage_writer,
            )
            .map_err(|err| {
                error!("Failed to get commitment results and write to storage: {err}");
                BatcherError::InternalError
            })?;
        Ok(())
    }

    async fn write_commitment_results_and_add_new_task(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        optional_state_diff_commitment: Option<StateDiffCommitment>,
    ) -> BatcherResult<()> {
        self.get_commitment_results_and_write_to_storage()?;
        self.commitment_manager
            .add_commitment_task(
                height,
                state_diff,
                optional_state_diff_commitment,
                &self.config.static_config.first_block_with_partial_block_hash,
                self.storage_reader.clone(),
                &mut self.storage_writer,
            )
            .await
            .expect("The commitment offset unexpectedly doesn't match the given block height.");
        Ok(())
    }
}

/// Logs the result of the transactions execution in the proposal.
fn log_txs_execution_result(
    proposal_id: ProposalId,
    result: &Result<BlockExecutionArtifacts, Arc<BlockBuilderError>>,
) {
    if let Ok(block_artifacts) = result {
        let execution_infos = block_artifacts
            .execution_data
            .execution_infos_and_signatures
            .iter()
            .map(|(tx_hash, (info, _sig))| (tx_hash, info));
        let rejected_hashes = &block_artifacts.execution_data.rejected_tx_hashes;

        // Estimate capacity: base message + (hash + status) per transaction
        // TransactionHash is 66 chars (0x + 64 hex), status is ~12 chars, separator is 4 chars
        // Total per transaction: ~82 chars
        const CHARS_PER_TX: usize = 82;
        const BASE_CAPACITY: usize = 80; // Base message length
        let total_txs = execution_infos.len() + rejected_hashes.len();
        let estimated_capacity = BASE_CAPACITY + total_txs * CHARS_PER_TX;

        let mut log_msg = String::with_capacity(estimated_capacity);
        let _ = write!(
            &mut log_msg,
            "Finished generating proposal {} with {} transactions",
            proposal_id,
            execution_infos.len(),
        );

        for (tx_hash, info) in execution_infos {
            let status = if info.revert_error.is_some() { "Reverted" } else { "Successful" };
            let _ = write!(&mut log_msg, ", {tx_hash}: {status}");
        }

        for tx_hash in rejected_hashes {
            let _ = write!(&mut log_msg, ", {tx_hash}: Rejected");
        }

        info!("{}", log_msg);
    }
}

pub async fn create_batcher(
    config: BatcherConfig,
    committer_client: SharedCommitterClient,
    mempool_client: SharedMempoolClient,
    l1_provider_client: SharedL1ProviderClient,
    class_manager_client: SharedClassManagerClient,
    pre_confirmed_cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    config_manager_client: SharedConfigManagerClient,
) -> Batcher {
    let storage_reader_server_config = ServerConfig {
        static_config: config.static_config.storage_reader_server_static_config.clone(),
        dynamic_config: config.dynamic_config.storage_reader_server_dynamic_config.clone(),
    };
    let (storage_reader, storage_writer, storage_reader_server) =
        open_storage_with_metric_and_server(
            config.static_config.storage.clone(),
            &BATCHER_STORAGE_OPEN_READ_TRANSACTIONS,
            storage_reader_server_config,
        )
        .expect("Failed to open batcher's storage");

    let storage_reader_server_handle =
        GenericStorageReaderServer::spawn_if_enabled(storage_reader_server);

    let execute_config = &config.static_config.block_builder_config.execute_config;
    let worker_pool = Arc::new(WorkerPool::start(execute_config));
    let pre_confirmed_block_writer_factory = Box::new(PreconfirmedBlockWriterFactory {
        config: config.static_config.pre_confirmed_block_writer_config,
        cende_client: pre_confirmed_cende_client,
    });
    let block_builder_factory = Box::new(BlockBuilderFactory {
        block_builder_config: config.static_config.block_builder_config.clone(),
        storage_reader: storage_reader.clone(),
        contract_class_manager: ContractClassManager::start(
            config.static_config.contract_class_manager_config.clone(),
        ),
        class_manager_client: class_manager_client.clone(),
        worker_pool,
    });
    let storage_reader = Arc::new(storage_reader);
    let storage_writer = Box::new(storage_writer);
    let transaction_converter = TransactionConverter::new(
        class_manager_client,
        config.static_config.storage.db_config.chain_id.clone(),
    );

    let commitment_manager = CommitmentManager::create_commitment_manager(
        &config.static_config.commitment_manager_config,
        storage_reader.clone(),
        committer_client.clone(),
    )
    .await;

    Batcher::new(
        config,
        storage_reader,
        storage_writer,
        committer_client,
        l1_provider_client,
        mempool_client,
        transaction_converter,
        config_manager_client,
        block_builder_factory,
        pre_confirmed_block_writer_factory,
        commitment_manager,
        storage_reader_server_handle,
    )
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReader: Send + Sync {
    /// Returns the next height for which the batcher stores state diff for.
    /// This is the next height the batcher should work on (during validate/proposal).
    fn state_diff_height(&self) -> StorageResult<BlockNumber>;

    /// Returns the first height the committer has finished calculating commitments for.
    fn global_root_height(&self) -> StorageResult<BlockNumber>;

    fn global_root(&self, height: BlockNumber) -> StorageResult<Option<GlobalRoot>>;

    fn get_state_diff(&self, height: BlockNumber) -> StorageResult<Option<ThinStateDiff>>;

    /// Returns the state diff that undoes the state diff at the given height.
    /// Ignores deprecated_declared_classes.
    fn reversed_state_diff(
        &self,
        height: BlockNumber,
    ) -> apollo_storage::StorageResult<ThinStateDiff>;

    fn get_block_hash(&self, height: BlockNumber) -> StorageResult<Option<BlockHash>>;

    fn get_parent_hash_and_partial_block_hash_components(
        &self,
        height: BlockNumber,
    ) -> StorageResult<(Option<BlockHash>, Option<PartialBlockHashComponents>)>;
}

impl BatcherStorageReader for StorageReader {
    fn state_diff_height(&self) -> StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_state_marker()
    }

    fn global_root_height(&self) -> StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_global_root_marker()
    }

    fn global_root(&self, height: BlockNumber) -> StorageResult<Option<GlobalRoot>> {
        self.begin_ro_txn()?.get_global_root(&height)
    }

    fn get_state_diff(&self, height: BlockNumber) -> StorageResult<Option<ThinStateDiff>> {
        self.begin_ro_txn()?.get_state_diff(height)
    }

    fn reversed_state_diff(
        &self,
        height: BlockNumber,
    ) -> apollo_storage::StorageResult<ThinStateDiff> {
        let state_target = StateNumber::right_before_block(height);
        let txn = self.begin_ro_txn()?;

        let ThinStateDiff {
            deployed_contracts,
            storage_diffs,
            class_hash_to_compiled_class_hash,
            nonces,
            ..
        } = txn.get_state_diff(height)?.ok_or_else(|| StorageError::MissingObject {
            object_name: "state diff".to_string(),
            height,
        })?;

        let state_reader = txn.get_state_reader()?;

        // In the following maps, set empty values to zero.
        let mut reversed_deployed_contracts = IndexMap::new();
        for contract_address in deployed_contracts.keys() {
            let class_hash =
                state_reader.get_class_hash_at(state_target, contract_address)?.unwrap_or_default();
            reversed_deployed_contracts.insert(*contract_address, class_hash);
        }

        let mut reversed_storage_diffs = IndexMap::new();
        for (contract_address, contract_diffs) in storage_diffs {
            let mut reversed_contract_diffs = IndexMap::new();
            for key in contract_diffs.keys() {
                let value = state_reader.get_storage_at(state_target, &contract_address, key)?;
                reversed_contract_diffs.insert(*key, value);
            }
            reversed_storage_diffs.insert(contract_address, reversed_contract_diffs);
        }

        let mut reversed_class_hash_to_compiled_class_hash = IndexMap::new();
        for class_hash in class_hash_to_compiled_class_hash.keys() {
            let compiled_class_hash = state_reader
                .get_compiled_class_hash_at(state_target, class_hash)?
                .unwrap_or_default();
            reversed_class_hash_to_compiled_class_hash.insert(*class_hash, compiled_class_hash);
        }

        let mut reversed_nonces = IndexMap::new();
        for contract_address in nonces.keys() {
            let nonce =
                state_reader.get_nonce_at(state_target, contract_address)?.unwrap_or_default();
            reversed_nonces.insert(*contract_address, nonce);
        }

        Ok(ThinStateDiff {
            deployed_contracts: reversed_deployed_contracts,
            storage_diffs: reversed_storage_diffs,
            class_hash_to_compiled_class_hash: reversed_class_hash_to_compiled_class_hash,
            nonces: reversed_nonces,
            deprecated_declared_classes: Default::default(),
        })
    }

    fn get_block_hash(&self, height: BlockNumber) -> StorageResult<Option<BlockHash>> {
        self.begin_ro_txn()?.get_block_hash(&height)
    }

    fn get_parent_hash_and_partial_block_hash_components(
        &self,
        height: BlockNumber,
    ) -> StorageResult<(Option<BlockHash>, Option<PartialBlockHashComponents>)> {
        let txn = self.begin_ro_txn()?;
        let parent_hash = match height.prev() {
            None => Some(BlockHash::GENESIS_PARENT_HASH),
            Some(parent_height) => txn.get_block_hash(&parent_height)?,
        };
        let partial_block_hash_components = txn.get_partial_block_hash_components(&height)?;
        Ok((parent_hash, partial_block_hash_components))
    }
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageWriter: Send + Sync {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        storage_commitment_block_hash: StorageCommitmentBlockHash,
    ) -> StorageResult<()>;

    fn revert_block(&mut self, height: BlockNumber);

    /// Sets the global root and block hash (unless it's None) for the given height.
    /// Increments the block hash marker by 1.
    /// Block hash is optional because for old blocks, the block hash was set separately.
    fn set_global_root_and_block_hash(
        &mut self,
        height: BlockNumber,
        global_root: GlobalRoot,
        block_hash: Option<BlockHash>,
    ) -> StorageResult<()>;

    fn set_block_hash(&mut self, height: BlockNumber, block_hash: BlockHash) -> StorageResult<()>;
}

impl BatcherStorageWriter for StorageWriter {
    fn commit_proposal(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        storage_commitment_block_hash: StorageCommitmentBlockHash,
    ) -> StorageResult<()> {
        // TODO(AlonH): write casms.
        let mut txn = self.begin_rw_txn()?.append_state_diff(height, state_diff)?;
        match storage_commitment_block_hash {
            StorageCommitmentBlockHash::ParentHash(parent_block_hash) => {
                if let Some(parent_block_number) = height.prev() {
                    txn = txn.set_block_hash(&parent_block_number, parent_block_hash)?
                }
            }
            StorageCommitmentBlockHash::Partial(partial_block_hash_components) => {
                txn =
                    txn.set_partial_block_hash_components(&height, &partial_block_hash_components)?
            }
        }
        txn.commit()
    }

    // This function will panic if there is a storage failure to revert the block.
    fn revert_block(&mut self, height: BlockNumber) {
        revert_block(self, height);
    }

    fn set_global_root_and_block_hash(
        &mut self,
        height: BlockNumber,
        global_root: GlobalRoot,
        block_hash: Option<BlockHash>,
    ) -> StorageResult<()> {
        let mut txn = self
            .begin_rw_txn()?
            .set_global_root(&height, global_root)?
            .checked_increment_global_root_marker(height)?;
        if let Some(block_hash) = block_hash {
            txn = txn.set_block_hash(&height, block_hash)?;
        }
        txn.commit()
    }

    fn set_block_hash(&mut self, height: BlockNumber, block_hash: BlockHash) -> StorageResult<()> {
        self.begin_rw_txn()?.set_block_hash(&height, block_hash)?.commit()
    }
}

#[async_trait]
impl ComponentStarter for Batcher {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        let storage_height = self
            .storage_reader
            .state_diff_height()
            .expect("Failed to get height from storage during batcher creation.");
        let global_root_height = self
            .storage_reader
            .global_root_height()
            .expect("Failed to get global roots height from storage during batcher creation.");

        self.commitment_manager
            .add_missing_commitment_tasks(
                storage_height,
                &self.config,
                self.storage_reader.clone(),
                &mut self.storage_writer,
            )
            .await;

        register_metrics(storage_height, global_root_height);
    }
}

/// When committing a block into storage, in `commit_proposal_and_block`, we either set the
/// parent hash for old blocks (pre 0.13.2) coming from sync, or the partial block hash components
/// for new blocks.
#[derive(Debug, PartialEq)]
pub enum StorageCommitmentBlockHash {
    ParentHash(BlockHash),
    Partial(PartialBlockHashComponents),
}
