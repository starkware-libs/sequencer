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
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::error;

use crate::config::BatcherConfig;
use crate::proposal_manager::{BlockBuilderFactory, ProposalManager, ProposalManagerError};

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
        self.proposal_manager.start_height(input.height).map_err(|err| match err {
            ProposalManagerError::AlreadyWorkingOnHeight { active_height, new_height } => {
                BatcherError::AlreadyWorkingOnHeight { active_height, new_height }
            }
            ProposalManagerError::HeightAlreadyPassed { storage_height, requested_height } => {
                BatcherError::HeightAlreadyPassed { storage_height, requested_height }
            }
            ProposalManagerError::StorageError(err) => {
                error!("{}", err);
                BatcherError::InternalError
            }
            ProposalManagerError::StorageNotSynced { storage_height, requested_height } => {
                error!("{}", err);
                BatcherError::StorageNotSynced { storage_height, requested_height }
            }
            ProposalManagerError::AlreadyGeneratingProposal { .. }
            | ProposalManagerError::GetContentOnNonBuildProposal { .. }
            | ProposalManagerError::MempoolError { .. }
            | ProposalManagerError::NoActiveHeight
            | ProposalManagerError::ProposalAlreadyExists { .. }
            | ProposalManagerError::ProposalNotFound { .. }
            | ProposalManagerError::StreamExhausted => {
                unreachable!("Shouldn't happen here: {}", err)
            }
        })
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
            .map_err(|err| match err {
                ProposalManagerError::AlreadyGeneratingProposal {
                    current_generating_proposal_id,
                    new_proposal_id,
                } => BatcherError::ServerBusy {
                    active_proposal_id: current_generating_proposal_id,
                    new_proposal_id,
                },
                ProposalManagerError::ProposalAlreadyExists { proposal_id } => {
                    BatcherError::ProposalAlreadyExists { proposal_id }
                }
                ProposalManagerError::MempoolError(..) => {
                    error!("MempoolError: {}", err);
                    BatcherError::InternalError
                }
                ProposalManagerError::NoActiveHeight => BatcherError::NoActiveHeight,
                ProposalManagerError::AlreadyWorkingOnHeight { .. }
                | ProposalManagerError::GetContentOnNonBuildProposal { .. }
                | ProposalManagerError::HeightAlreadyPassed { .. }
                | ProposalManagerError::ProposalNotFound { .. }
                | ProposalManagerError::StorageError(..)
                | ProposalManagerError::StorageNotSynced { .. }
                | ProposalManagerError::StreamExhausted => {
                    unreachable!("Shouldn't happen here: {}", err)
                }
            })?;
        Ok(())
    }

    pub async fn get_proposal_content(
        &mut self,
        get_proposal_content_input: GetProposalContentInput,
    ) -> BatcherResult<GetProposalContentResponse> {
        let content = self
            .proposal_manager
            .get_proposal_content(get_proposal_content_input.proposal_id)
            .await
            .map_err(|err| match err {
                ProposalManagerError::StreamExhausted => BatcherError::StreamExhausted,
                ProposalManagerError::GetContentOnNonBuildProposal { proposal_id } => {
                    BatcherError::GetContentOnNonBuildProposal { proposal_id }
                }
                ProposalManagerError::ProposalNotFound { proposal_id } => {
                    BatcherError::ProposalNotFound { proposal_id }
                }
                ProposalManagerError::AlreadyGeneratingProposal { .. }
                | ProposalManagerError::AlreadyWorkingOnHeight { .. }
                | ProposalManagerError::HeightAlreadyPassed { .. }
                | ProposalManagerError::MempoolError(..)
                | ProposalManagerError::NoActiveHeight
                | ProposalManagerError::ProposalAlreadyExists { .. }
                | ProposalManagerError::StorageError(..)
                | ProposalManagerError::StorageNotSynced { .. } => {
                    unreachable!("Shouldn't happen here: {}", err)
                }
            })?;
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

impl ComponentStarter for Batcher {}