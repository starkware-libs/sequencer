use std::collections::HashMap;
use std::sync::Arc;

use blockifier::state::global_cache::GlobalContractCache;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_batcher_types::batcher_types::{
    BatcherResult,
    BuildProposalInput,
    DecisionReachedInput,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalId,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateProposalInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::CommitBlockArgs;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use tracing::{debug, error, info, instrument, trace};

use crate::block_builder::BlockBuilderFactory;
use crate::config::BatcherConfig;
use crate::proposal_manager::{
    BuildProposalError,
    GetProposalResultError,
    ProposalManager,
    ProposalManagerTrait,
    ProposalOutput,
    StartHeightError,
};
use crate::transaction_provider::{DummyL1ProviderClient, ProposeTransactionProvider};

struct Proposal {
    tx_stream: OutputStream,
}

pub struct Batcher {
    pub config: BatcherConfig,
    pub storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    pub storage_writer: Box<dyn BatcherStorageWriterTrait>,
    pub mempool_client: SharedMempoolClient,
    proposal_manager: Box<dyn ProposalManagerTrait>,
    proposals: HashMap<ProposalId, Proposal>,
}

impl Batcher {
    pub(crate) fn new(
        config: BatcherConfig,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
        storage_writer: Box<dyn BatcherStorageWriterTrait>,
        mempool_client: SharedMempoolClient,
        proposal_manager: Box<dyn ProposalManagerTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            storage_reader,
            storage_writer,
            mempool_client,
            proposal_manager,
            proposals: HashMap::new(),
        }
    }

    pub async fn start_height(&mut self, input: StartHeightInput) -> BatcherResult<()> {
        self.proposals.clear();
        self.proposal_manager.start_height(input.height).await.map_err(BatcherError::from)
    }

    #[instrument(skip(self), err)]
    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> BatcherResult<()> {
        let proposal_id = build_proposal_input.proposal_id;
        let deadline =
            tokio::time::Instant::from_std(build_proposal_input.deadline_as_instant().map_err(
                |_| BatcherError::TimeToDeadlineError { deadline: build_proposal_input.deadline },
            )?);

        let (tx_sender, tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let tx_provider = ProposeTransactionProvider::new(
            self.config.transaction_provider_config.clone(),
            self.mempool_client.clone(),
            // TODO: use a real L1 provider client.
            Arc::new(DummyL1ProviderClient),
        );

        self.proposal_manager
            .build_block_proposal(
                build_proposal_input.proposal_id,
                build_proposal_input.retrospective_block_hash,
                deadline,
                tx_sender,
                tx_provider,
            )
            .await
            .map_err(BatcherError::from)?;

        let tx_stream = tx_receiver;
        self.proposals.insert(proposal_id, Proposal { tx_stream });
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn validate_proposal(
        &mut self,
        validate_proposal_input: ValidateProposalInput,
    ) -> BatcherResult<()> {
        todo!();
    }

    #[instrument(skip(self), err)]
    pub async fn send_proposal_content(
        &mut self,
        send_proposal_content_input: SendProposalContentInput,
    ) -> BatcherResult<SendProposalContentResponse> {
        todo!();
    }

    #[instrument(skip(self), err)]
    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let proposal_id = get_proposal_content_input.proposal_id;

        let tx_stream = &mut self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?
            .tx_stream;

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
        self.proposals.remove(&proposal_id);
        let proposal_commitment =
            self.proposal_manager.get_executed_proposal_commitment(proposal_id).await?;
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(proposal_commitment),
        })
    }

    #[instrument(skip(self), err)]
    pub async fn decision_reached(&mut self, input: DecisionReachedInput) -> BatcherResult<()> {
        let proposal_id = input.proposal_id;
        let proposal_output = self.proposal_manager.take_proposal_result(proposal_id).await?;
        let ProposalOutput { state_diff, nonces: address_to_nonce, tx_hashes, .. } =
            proposal_output;
        // TODO: Keep the height from start_height or get it from the input.
        let height = self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })?;
        info!(
            "Committing proposal {} at height {} and notifying mempool of the block.",
            proposal_id, height
        );
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

    let block_builder_factory = Arc::new(BlockBuilderFactory {
        block_builder_config: config.block_builder_config.clone(),
        storage_reader: storage_reader.clone(),
        global_class_hash_to_class: GlobalContractCache::new(config.global_contract_cache_size),
    });
    let storage_reader = Arc::new(storage_reader);
    let storage_writer = Box::new(storage_writer);
    let proposal_manager =
        Box::new(ProposalManager::new(block_builder_factory, storage_reader.clone()));
    Batcher::new(config, storage_reader, storage_writer, mempool_client, proposal_manager)
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

// TODO: Make this work with streams.
type OutputStream = tokio::sync::mpsc::UnboundedReceiver<Transaction>;
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

impl From<StartHeightError> for BatcherError {
    fn from(err: StartHeightError) -> Self {
        match err {
            StartHeightError::AlreadyWorkingOnHeight { active_height, new_height } => {
                BatcherError::AlreadyWorkingOnHeight { active_height, new_height }
            }
            StartHeightError::HeightAlreadyPassed { storage_height, requested_height } => {
                BatcherError::HeightAlreadyPassed { storage_height, requested_height }
            }
            StartHeightError::StorageError(err) => {
                error!("{}", err);
                BatcherError::InternalError
            }
            StartHeightError::StorageNotSynced { storage_height, requested_height } => {
                BatcherError::StorageNotSynced { storage_height, requested_height }
            }
        }
    }
}

impl From<BuildProposalError> for BatcherError {
    fn from(err: BuildProposalError) -> Self {
        match err {
            BuildProposalError::AlreadyGeneratingProposal {
                current_generating_proposal_id,
                new_proposal_id,
            } => BatcherError::ServerBusy {
                active_proposal_id: current_generating_proposal_id,
                new_proposal_id,
            },
            BuildProposalError::BlockBuilderError(..) => BatcherError::InternalError,
            BuildProposalError::NoActiveHeight => BatcherError::NoActiveHeight,
            BuildProposalError::ProposalAlreadyExists { proposal_id } => {
                BatcherError::ProposalAlreadyExists { proposal_id }
            }
        }
    }
}

impl From<GetProposalResultError> for BatcherError {
    fn from(err: GetProposalResultError) -> Self {
        match err {
            GetProposalResultError::BlockBuilderError(..) => BatcherError::InternalError,
            GetProposalResultError::ProposalDoesNotExist { proposal_id } => {
                BatcherError::ExecutedProposalNotFound { proposal_id }
            }
        }
    }
}

impl ComponentStarter for Batcher {}
