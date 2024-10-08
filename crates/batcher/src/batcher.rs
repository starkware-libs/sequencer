use std::sync::Arc;

use blockifier::blockifier::block::BlockNumberHashPair;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::StateStorageReader;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{BatcherResult, BuildProposalInput, StartHeightInput};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tracing::error;

use crate::block_builder::{BlockBuilderFactoryTrait, BlockBuilderResult, BlockBuilderTrait};
use crate::config::BatcherConfig;
use crate::proposal_manager::{ProposalManager, ProposalManagerError, ProposalManagerTrait};

pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    pub storage: Arc<dyn BatcherStorageReaderTrait>,
    proposal_manager: Box<dyn ProposalManagerTrait>,
}

// TODO(Yael 7/10/2024): remove DummyBlockBuilderFactory and pass the real BlockBuilderFactory
struct DummyBlockBuilderFactory {}

impl BlockBuilderFactoryTrait for DummyBlockBuilderFactory {
    fn create_block_builder(
        &self,
        _height: BlockNumber,
        _retrospective_block_hash: Option<BlockNumberHashPair>,
    ) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
        todo!()
    }
}

impl Batcher {
    fn new(
        config: BatcherConfig,
        mempool_client: SharedMempoolClient,
        storage: Arc<dyn BatcherStorageReaderTrait>,
        proposal_manager: Box<dyn ProposalManagerTrait>,
    ) -> Self {
        Self {
            config: config.clone(),
            mempool_client: mempool_client.clone(),
            storage: storage.clone(),
            proposal_manager,
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
            | ProposalManagerError::MempoolError { .. }
            | ProposalManagerError::NoActiveHeight
            | ProposalManagerError::ProposalAlreadyExists { .. }
            | ProposalManagerError::BlockBuilderError(..) => {
                unreachable!("Shouldn't happen here: {}", err)
            }
        })
    }

    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> BatcherResult<()> {
        // TODO: Save the receiver as a stream for later use.
        let (content_sender, _content_receiver) =
            tokio::sync::mpsc::channel(self.config.outstream_content_buffer_size);
        let deadline =
            tokio::time::Instant::from_std(build_proposal_input.deadline_as_instant().map_err(
                |_| BatcherError::TimeToDeadlineError { deadline: build_proposal_input.deadline },
            )?);
        self.proposal_manager
            .build_block_proposal(
                build_proposal_input.proposal_id,
                build_proposal_input.retrospective_block_hash,
                deadline,
                content_sender,
            )
            .await
            .map_err(|err| match err {
                ProposalManagerError::AlreadyGeneratingProposal {
                    current_generating_proposal_id,
                    new_proposal_id,
                } => BatcherError::ServerBusy {
                    active_proposal_id: current_generating_proposal_id,
                    new_proposal_id,
                },
                ProposalManagerError::BlockBuilderError(..) => BatcherError::InternalError,
                ProposalManagerError::ProposalAlreadyExists { proposal_id } => {
                    BatcherError::ProposalAlreadyExists { proposal_id }
                }
                ProposalManagerError::MempoolError(..) => {
                    error!("MempoolError: {}", err);
                    BatcherError::InternalError
                }
                ProposalManagerError::NoActiveHeight => BatcherError::NoActiveHeight,
                ProposalManagerError::AlreadyWorkingOnHeight { .. }
                | ProposalManagerError::HeightAlreadyPassed { .. }
                | ProposalManagerError::StorageError(..)
                | ProposalManagerError::StorageNotSynced { .. } => {
                    unreachable!("Shouldn't happen here: {}", err)
                }
            })?;
        Ok(())
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");
    let storage_reader = Arc::new(storage_reader);
    let proposal_manager = Box::new(ProposalManager::new(
        config.proposal_manager.clone(),
        mempool_client.clone(),
        Arc::new(DummyBlockBuilderFactory {}),
        storage_reader.clone(),
    ));
    Batcher::new(config, mempool_client, storage_reader, proposal_manager)
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
