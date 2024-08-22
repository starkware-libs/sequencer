use std::sync::Arc;

use async_trait::async_trait;
use starknet_batcher_types::batcher_types::BuildProposalInput;
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::config::BatcherConfig;
use crate::proposal_manager::{BlockBuilderFactory, ProposalManager, ProposalManagerError};

pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    proposal_manager: ProposalManager,
}

impl Batcher {
    pub fn new(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Self {
        Self {
            config: config.clone(),
            mempool_client: mempool_client.clone(),
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
            })?;
        Ok(())
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    Batcher::new(config, mempool_client)
}

#[async_trait]
impl ComponentStarter for Batcher {}
