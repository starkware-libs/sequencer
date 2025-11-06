use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use starknet_committer::hash_function::hash::StateRoots;

use crate::Committer;

pub type LocalCommitterServer =
    ConcurrentLocalComponentServer<Committer, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl ComponentRequestHandler<CommitterRequest, CommitterResponse> for Committer {
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::CommitBlock { state_diff: _, prev_state_roots: _ } => {
                CommitterResponse::CommitBlock(
                    Ok(StateRoots::default()), // Placeholder implementation
                )
            }
        }
    }
}
