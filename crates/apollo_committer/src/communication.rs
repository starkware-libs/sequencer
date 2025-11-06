use apollo_committer_types::{CommitterRequest, CommitterResponse, StateRoots};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::ApolloCommitter;

pub type LocalCommitterServer =
    ConcurrentLocalComponentServer<ApolloCommitter, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl ComponentRequestHandler<CommitterRequest, CommitterResponse> for ApolloCommitter {
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::CommitStateDiff { state_diff: _, prev_state_roots: _ } => {
                CommitterResponse::CommitStateDiff(
                    Ok(StateRoots::default()), // Placeholder implementation
                )
            }
        }
    }
}
