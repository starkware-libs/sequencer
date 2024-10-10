use std::collections::HashMap;
use std::sync::Arc;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::ChainInfo;
use blockifier::state::global_cache::GlobalContractCache;
use blockifier::versioned_constants::VersionedConstantsOverrides;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
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
    StartHeightInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::{error, instrument};

use crate::block_builder::{BlockBuilderFactory, ExecutionConfig};
use crate::config::BatcherConfig;
use crate::proposal_manager::{
    BuildProposalError,
    GetDoneProposalError,
    ProposalManager,
    ProposalManagerTrait,
    StartHeightError,
};

struct Proposal {
    content_stream: OutputStream,
}

pub struct Batcher {
    pub config: BatcherConfig,
    pub storage_reader: Arc<dyn BatcherStorageReaderTrait>,
    pub storage_writer: Box<dyn BatcherStorageWriterTrait>,
    proposal_manager: Box<dyn ProposalManagerTrait>,
    proposals: HashMap<ProposalId, Proposal>,
}

impl Batcher {
    pub(crate) fn new(
        config: BatcherConfig,
        storage_reader: Arc<dyn BatcherStorageReaderTrait>,
        storage_writer: Box<dyn BatcherStorageWriterTrait>,
        proposal_manager: Box<dyn ProposalManagerTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            storage_reader,
            storage_writer,
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

        self.proposal_manager
            .build_block_proposal(
                build_proposal_input.proposal_id,
                build_proposal_input.retrospective_block_hash,
                deadline,
                tx_sender,
            )
            .await
            .map_err(BatcherError::from)?;

        let content_stream = tx_receiver;
        self.proposals.insert(proposal_id, Proposal { content_stream });
        Ok(())
    }

    #[instrument(skip(self), err)]
    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let proposal_id = get_proposal_content_input.proposal_id;
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(BatcherError::ProposalNotFound { proposal_id })?;

        let mut txs = Vec::new();
        if proposal
            .content_stream
            .recv_many(&mut txs, self.config.outstream_content_buffer_size)
            .await
            == 0
        {
            // TODO: send `Finished` and then exhausted.
            return Err(BatcherError::StreamExhausted);
        }

        Ok(GetProposalContentResponse { content: GetProposalContent::Txs(txs) })
    }

    #[instrument(skip(self), err)]
    pub async fn decision_reached(&mut self, input: DecisionReachedInput) -> BatcherResult<()> {
        let proposal_id = input.proposal_id;
        let state_diff = self.proposal_manager.get_done_proposal(proposal_id).await??;
        // TODO: Keep the height from start_height or get it from the input.
        let height = self.storage_reader.height().map_err(|err| {
            error!("Failed to get height from storage: {}", err);
            BatcherError::InternalError
        })?;
        self.storage_writer.commit_proposal(height, state_diff).map_err(|err| {
            error!("Failed to commit proposal to storage: {}", err);
            BatcherError::InternalError
        })
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");

    // TODO(Arni): use real config - add as part of batcher config.
    let execution_config = ExecutionConfig {
        chain_info: ChainInfo::default(),
        execute_config: TransactionExecutorConfig::default(),
        bouncer_config: BouncerConfig::empty(),
        sequencer_address: ContractAddress::default(),
        use_kzg_da: true,
        tx_chunk_size: 100,
        versioned_constants_overrides: VersionedConstantsOverrides {
            validate_max_n_steps: 1000000,
            max_recursion_depth: 50,
            invoke_tx_max_n_steps: 4000000,
        },
    };
    let cache_size = 100;

    let block_builder_factory = Arc::new(BlockBuilderFactory {
        execution_config,
        storage_reader: storage_reader.clone(),
        global_class_hash_to_class: GlobalContractCache::new(cache_size),
    });
    let storage_reader = Arc::new(storage_reader);
    let storage_writer = Box::new(storage_writer);
    let proposal_manager = Box::new(ProposalManager::new(
        config.proposal_manager.clone(),
        mempool_client.clone(),
        block_builder_factory,
        storage_reader.clone(),
    ));
    Batcher::new(config, storage_reader, storage_writer, proposal_manager)
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
            BuildProposalError::MempoolError(..) => BatcherError::InternalError,
            BuildProposalError::NoActiveHeight => BatcherError::NoActiveHeight,
            BuildProposalError::ProposalAlreadyExists { proposal_id } => {
                BatcherError::ProposalAlreadyExists { proposal_id }
            }
        }
    }
}

impl From<GetDoneProposalError> for BatcherError {
    fn from(err: GetDoneProposalError) -> Self {
        match err {
            GetDoneProposalError::ProposalDoesNotExist { proposal_id } => {
                BatcherError::DoneProposalNotFound { proposal_id }
            }
        }
    }
}

impl ComponentStarter for Batcher {}
