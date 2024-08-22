use std::sync::Arc;

use async_trait::async_trait;
use starknet_batcher_types::batcher_types::BuildProposalInput;
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::config::BatcherConfig;
use crate::proposals_manager::{
    BlockBuilderFactoryImpl,
    ProposalsManager,
    ProposalsManagerError,
    ProposalsManagerTrait,
};

pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    proposals_manager: Box<dyn ProposalsManagerTrait>,
}

impl Batcher {
    pub fn new(
        config: BatcherConfig,
        mempool_client: SharedMempoolClient,
        proposals_manager: impl ProposalsManagerTrait + 'static,
    ) -> Self {
        Self {
            config: config.clone(),
            mempool_client: mempool_client.clone(),
            proposals_manager: Box::new(proposals_manager),
        }
    }

    pub async fn build_proposal(
        &mut self,
        build_proposal_input: BuildProposalInput,
    ) -> Result<(), BatcherError> {
        // TODO: Save the stream for later use.
        let _tx_stream =
            self.proposals_manager
                .generate_block_proposal(
                    tokio::time::Instant::from_std(
                        build_proposal_input.deadline_as_instant().map_err(|_| {
                            BatcherError::TimeToDeadlineError {
                                deadline: build_proposal_input.deadline,
                            }
                        })?,
                    ),
                    build_proposal_input.height,
                )
                .await
                .map_err(|err| match err {
                    ProposalsManagerError::AlreadyGeneratingProposal { .. } => {
                        BatcherError::AlreadyGeneratingProposal
                    }
                    ProposalsManagerError::InternalError
                    | ProposalsManagerError::MempoolError(..) => BatcherError::InternalError,
                })?;
        Ok(())
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    Batcher::new(
        config.clone(),
        mempool_client.clone(),
        ProposalsManager::new(
            config.proposals_manager.clone(),
            mempool_client.clone(),
            Arc::new(BlockBuilderFactoryImpl {}),
        ),
    )
}

#[async_trait]
impl ComponentStarter for Batcher {}
