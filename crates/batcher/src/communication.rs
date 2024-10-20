use async_trait::async_trait;
use starknet_batcher_types::communication::{
    BatcherRequest,
    BatcherRequestAndResponseSender,
    BatcherResponse,
};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_server::LocalComponentServer;
use tokio::sync::mpsc::Receiver;

use crate::batcher::Batcher;

pub type LocalBatcherServer = LocalComponentServer<Batcher, BatcherRequest, BatcherResponse>;

pub fn create_local_batcher_server(
    batcher: Batcher,
    rx_batcher: Receiver<BatcherRequestAndResponseSender>,
) -> LocalBatcherServer {
    LocalComponentServer::new(batcher, rx_batcher)
}

#[async_trait]
impl ComponentRequestHandler<BatcherRequest, BatcherResponse> for Batcher {
    async fn handle_request(&mut self, request: BatcherRequest) -> BatcherResponse {
        match request {
            BatcherRequest::BuildProposal(input) => {
                BatcherResponse::BuildProposal(self.build_proposal(input).await)
            }
            BatcherRequest::GetProposalContent(input) => {
                BatcherResponse::GetProposalContent(self.get_proposal_content(input).await)
            }
            BatcherRequest::StartHeight(input) => {
                BatcherResponse::StartHeight(self.start_height(input).await)
            }
            BatcherRequest::DecisionReached(input) => {
                BatcherResponse::DecisionReached(self.decision_reached(input).await)
            }
            _ => unimplemented!(),
        }
    }
}
