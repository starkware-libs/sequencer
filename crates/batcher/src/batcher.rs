use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::{
    BatcherResult,
    BuildProposalInput,
    GetStreamContentInput,
    ProposalContentId,
    StreamContent,
    StreamId,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use crate::config::BatcherConfig;
use crate::proposals_manager::{
    BlockBuilderFactoryImpl,
    ProposalId,
    ProposalsManager,
    ProposalsManagerError,
    ProposalsManagerResult,
};

// TODO(Tsabary/Yael/Dafna): Replace with actual batcher code.
pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    proposals_manager: Box<dyn ProposalsManagerTrait>,
    proposal_id_marker: ProposalId,
    outbound_tx_streams: HashMap<StreamId, Arc<Mutex<BoxStream<'static, Transaction>>>>,
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
            proposal_id_marker: ProposalId::default(),
            outbound_tx_streams: HashMap::new(),
        }
    }

    #[instrument(skip(self), ret, err)]
    pub async fn build_proposal(
        &mut self,
        build_proposal_input: &BuildProposalInput,
    ) -> BatcherResult<()> {
        if self.outbound_tx_streams.contains_key(&build_proposal_input.stream_id) {
            return Err(BatcherError::StreamIdAlreadyExists {
                stream_id: build_proposal_input.stream_id,
            });
        }
        let proposal_id = self.proposal_id_marker;
        self.proposal_id_marker += 1;
        // TODO: Save the stream for later use.
        let tx_stream =
            self.proposals_manager
                .call_generate_block_proposal(
                    proposal_id,
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
        self.outbound_tx_streams
            .insert(build_proposal_input.stream_id, Arc::new(Mutex::new(tx_stream)));
        Ok(())
    }

    /// Returns the next item of content on this stream, blocking until it is available. When the
    /// stream is complete the function returns the proposal content ID.
    #[instrument(skip(self), ret, err)]
    pub async fn get_stream_content(
        &mut self,
        input: &GetStreamContentInput,
    ) -> BatcherResult<StreamContent> {
        let stream_id = input.stream_id;
        // If the stream is exhausted we need to remove it from the map.
        // In order to do that we need to drop the stream instance (HashMap::remove requires a ref
        // to the stream).
        {
            let stream = self
                .outbound_tx_streams
                .get_mut(&stream_id)
                .ok_or(BatcherError::StreamIdDoesNotExist { stream_id })?;

            if let Some(tx) = stream.lock().await.next().await {
                return Ok(StreamContent::Tx(tx));
            }
        }

        debug!("Stream is exhausted, removing from map");
        self.outbound_tx_streams.remove(&stream_id);
        return Ok(StreamContent::StreamEnd(ProposalContentId {
            // TODO: Populate with actual tx commitment.
            tx_commitment: TransactionCommitment::default(),
        }));
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

#[cfg_attr(test, automock)]
#[async_trait]
pub trait ProposalsManagerTrait: Send + Sync {
    async fn call_generate_block_proposal<'a>(
        &mut self,
        proposal_id: ProposalId,
        timeout: tokio::time::Instant,
        _height: BlockNumber,
    ) -> ProposalsManagerResult<BoxStream<'a, Transaction>>;
}

#[async_trait]
impl ProposalsManagerTrait for ProposalsManager {
    async fn call_generate_block_proposal<'a>(
        &mut self,
        proposal_id: ProposalId,
        timeout: tokio::time::Instant,
        height: BlockNumber,
    ) -> ProposalsManagerResult<BoxStream<'a, Transaction>> {
        Ok(self.generate_block_proposal(proposal_id, timeout, height).await?.boxed())
    }
}
