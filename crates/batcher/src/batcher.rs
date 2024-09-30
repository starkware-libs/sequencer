use std::sync::Arc;

#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::StateStorageReader;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{
    BatcherResult,
    BuildProposalInput,
    GetProposalContentInput,
    GetProposalContentResponse,
    StartHeightInput,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::starters::DefaultComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::error;

use crate::config::BatcherConfig;
use crate::proposal_manager::{
    BlockBuilderFactory,
    BuildProposalError,
    GetProposalContentError,
    ProposalManager,
    StartHeightError,
};

pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    pub storage: Arc<dyn BatcherStorageReaderTrait>,
    proposal_manager: ProposalManager,
}

impl Batcher {
    pub fn new(
        config: BatcherConfig,
        mempool_client: SharedMempoolClient,
        storage: Arc<dyn BatcherStorageReaderTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            mempool_client: mempool_client.clone(),
            storage: storage.clone(),
            proposal_manager: ProposalManager::new(
                config.proposal_manager.clone(),
                mempool_client.clone(),
                Arc::new(BlockBuilderFactory {}),
                storage,
            ),
        }
    }

    pub fn start_height(&mut self, input: StartHeightInput) -> BatcherResult<()> {
        self.proposal_manager.start_height(input.height).map_err(BatcherError::from)
    }

    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> BatcherResult<()> {
        let deadline =
            tokio::time::Instant::from_std(build_proposal_input.deadline_as_instant().map_err(
                |_| BatcherError::TimeToDeadlineError { deadline: build_proposal_input.deadline },
            )?);
        self.proposal_manager
            .build_block_proposal(build_proposal_input.proposal_id, deadline)
            .await
            .map_err(BatcherError::from)
    }

    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let content = self
            .proposal_manager
            .get_proposal_content(get_proposal_content_input.proposal_id)
            .await
            .map_err(BatcherError::from)?;
        Ok(GetProposalContentResponse { content })
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");
    Batcher::new(config, mempool_client, Arc::new(storage_reader))
}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber>;
}

impl BatcherStorageReaderTrait for papyrus_storage::StorageReader {
    fn height(&self) -> papyrus_storage::StorageResult<BlockNumber> {
        self.begin_ro_txn()?.get_state_marker()
    }
}

impl DefaultComponentStarter for Batcher {}

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
            BuildProposalError::MempoolError(..) => BatcherError::InternalError,
            BuildProposalError::NoActiveHeight => BatcherError::NoActiveHeight,
            BuildProposalError::ProposalAlreadyExists { proposal_id } => {
                BatcherError::ProposalAlreadyExists { proposal_id }
            }
        }
    }
}

impl From<GetProposalContentError> for BatcherError {
    fn from(err: GetProposalContentError) -> Self {
        match err {
            GetProposalContentError::GetContentOnNonBuildProposal { proposal_id } => {
                BatcherError::GetContentOnNonBuildProposal { proposal_id }
            }
            GetProposalContentError::ProposalNotFound { proposal_id } => {
                BatcherError::ProposalNotFound { proposal_id }
            }
            GetProposalContentError::StreamExhausted => BatcherError::StreamExhausted,
        }
    }
}
