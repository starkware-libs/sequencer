use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_batcher_types::batcher_types::BuildProposalInput;
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

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
            storage,
            proposal_manager: ProposalManager::new(
                config.proposal_manager.clone(),
                mempool_client.clone(),
                Arc::new(BlockBuilderFactory {}),
            ),
        }
    }

    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> Result<(), BatcherError> {
        // TODO: Save the receiver as a stream for later use.
        let (content_sender, _content_receiver) =
            tokio::sync::mpsc::channel(self.config.outstream_content_buffer_size);
        let deadline =
            tokio::time::Instant::from_std(build_proposal_input.deadline_as_instant().map_err(
                |_| BatcherError::TimeToDeadlineError { deadline: build_proposal_input.deadline },
            )?);
        self.proposal_manager
            .build_block_proposal(build_proposal_input.proposal_id, deadline, content_sender)
            .await
            .map_err(|err| match err {
                ProposalManagerError::AlreadyGeneratingProposal {
                    current_generating_proposal_id,
                    new_proposal_id,
                } => BatcherError::ServerBusy {
                    active_proposal_id: current_generating_proposal_id,
                    new_proposal_id,
                },
                ProposalManagerError::MempoolError(..) => BatcherError::InternalError,
                ProposalManagerError::ProposalAlreadyExists { proposal_id } => {
                    BatcherError::ProposalAlreadyExists { proposal_id }
                }
            })?;
        Ok(())
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");
    Batcher::new(config, mempool_client, Arc::new(storage_reader))
}

#[async_trait]
impl ComponentStarter for Batcher {}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {}

impl BatcherStorageReaderTrait for papyrus_storage::StorageReader {}
