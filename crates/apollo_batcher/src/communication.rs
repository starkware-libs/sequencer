use apollo_batcher_types::communication::{BatcherRequest, BatcherResponse};
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::batcher::Batcher;

pub type LocalBatcherServer =
    LocalComponentServer<BatcherCommunicationWrapper, BatcherRequest, BatcherResponse>;
pub type RemoteBatcherServer = RemoteComponentServer<BatcherRequest, BatcherResponse>;

/// Wraps the batcher to enable inbound async communication from other components.
pub struct BatcherCommunicationWrapper {
    batcher: Batcher,
    config_manager_client: SharedConfigManagerClient,
}

impl BatcherCommunicationWrapper {
    pub fn new(batcher: Batcher, config_manager_client: SharedConfigManagerClient) -> Self {
        BatcherCommunicationWrapper { batcher, config_manager_client }
    }

    /// Returns a reference to the inner batcher.
    pub fn batcher(&self) -> &Batcher {
        &self.batcher
    }

    async fn update_dynamic_config(&mut self) {
        let batcher_dynamic_config = self
            .config_manager_client
            .get_batcher_dynamic_config()
            .await
            .expect("Should be able to get batcher dynamic config");
        self.batcher.update_dynamic_config(batcher_dynamic_config);
    }
}

#[async_trait]
impl ComponentRequestHandler<BatcherRequest, BatcherResponse> for BatcherCommunicationWrapper {
    async fn handle_request(&mut self, request: BatcherRequest) -> BatcherResponse {
        // Update the dynamic config before handling the request.
        self.update_dynamic_config().await;
        match request {
            BatcherRequest::ProposeBlock(input) => {
                BatcherResponse::ProposeBlock(self.batcher.propose_block(input).await)
            }
            BatcherRequest::GetBlockHash(block_number) => {
                BatcherResponse::GetBlockHash(self.batcher.get_block_hash(block_number))
            }
            BatcherRequest::GetCurrentHeight => {
                BatcherResponse::GetCurrentHeight(self.batcher.get_height().await)
            }
            BatcherRequest::GetProposalContent(input) => {
                BatcherResponse::GetProposalContent(self.batcher.get_proposal_content(input).await)
            }
            BatcherRequest::StartHeight(input) => {
                BatcherResponse::StartHeight(self.batcher.start_height(input).await)
            }
            BatcherRequest::DecisionReached(input) => BatcherResponse::DecisionReached(
                self.batcher.decision_reached(input).await.map(Box::new),
            ),
            BatcherRequest::ValidateBlock(input) => {
                BatcherResponse::ValidateBlock(self.batcher.validate_block(input).await)
            }
            BatcherRequest::SendProposalContent(input) => BatcherResponse::SendProposalContent(
                self.batcher.send_proposal_content(input).await,
            ),
            BatcherRequest::AddSyncBlock(sync_block) => {
                BatcherResponse::AddSyncBlock(self.batcher.add_sync_block(sync_block).await)
            }
            BatcherRequest::RevertBlock(input) => {
                BatcherResponse::RevertBlock(self.batcher.revert_block(input).await)
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for BatcherCommunicationWrapper {
    async fn start(&mut self) {
        self.batcher.start().await;
    }
}
