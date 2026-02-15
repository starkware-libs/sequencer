use apollo_batcher_types::communication::{BatcherRequest, BatcherResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::batcher::Batcher;

pub type LocalBatcherServer = LocalComponentServer<Batcher, BatcherRequest, BatcherResponse>;
pub type RemoteBatcherServer = RemoteComponentServer<BatcherRequest, BatcherResponse>;

#[async_trait]
impl ComponentRequestHandler<BatcherRequest, BatcherResponse> for Batcher {
    async fn handle_request(&mut self, request: BatcherRequest) -> BatcherResponse {
        let dynamic_config = self
            .config_manager_client
            .get_batcher_dynamic_config()
            .await
            .expect("Should be able to get batcher dynamic config");
        self.update_dynamic_config(dynamic_config);

        match request {
            BatcherRequest::ProposeBlock(input) => {
                BatcherResponse::ProposeBlock(self.propose_block(input).await)
            }
            BatcherRequest::GetBlockHash(block_number) => {
                BatcherResponse::GetBlockHash(self.get_block_hash(block_number))
            }
            BatcherRequest::GetCurrentHeight => {
                BatcherResponse::GetCurrentHeight(self.get_height().await)
            }
            BatcherRequest::GetProposalContent(input) => {
                BatcherResponse::GetProposalContent(self.get_proposal_content(input).await)
            }
            BatcherRequest::StartHeight(input) => {
                BatcherResponse::StartHeight(self.start_height(input).await)
            }
            BatcherRequest::DecisionReached(input) => {
                BatcherResponse::DecisionReached(self.decision_reached(input).await.map(Box::new))
            }
            BatcherRequest::ValidateBlock(input) => {
                BatcherResponse::ValidateBlock(self.validate_block(input).await)
            }
            BatcherRequest::SendProposalContent(input) => {
                BatcherResponse::SendProposalContent(self.send_proposal_content(input).await)
            }
            BatcherRequest::AddSyncBlock(sync_block) => {
                BatcherResponse::AddSyncBlock(self.add_sync_block(sync_block).await)
            }
            BatcherRequest::RevertBlock(input) => {
                BatcherResponse::RevertBlock(self.revert_block(input).await)
            }
        }
    }
}
