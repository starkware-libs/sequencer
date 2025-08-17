use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::committer::Committer;

pub type LocalCommitterServer =
    LocalComponentServer<Committer, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl ComponentRequestHandler<CommitterRequest, CommitterResponse> for Committer {
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::Commit(input) => CommitterResponse::Commit(self.commit(input).await),
        }
    }
}
