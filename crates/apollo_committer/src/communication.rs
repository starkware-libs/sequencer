use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use starknet_committer::block_committer::commit::CommitBlockTrait;

use crate::committer::{Committer, StorageConstructor};

pub type LocalCommitterServer<S, CB> =
    LocalComponentServer<Committer<S, CB>, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl<S: StorageConstructor, CB: CommitBlockTrait>
    ComponentRequestHandler<CommitterRequest, CommitterResponse> for Committer<S, CB>
{
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::CommitBlock(commit_block_request) => {
                CommitterResponse::CommitBlock(self.commit_block(commit_block_request).await)
            }
            CommitterRequest::RevertBlock(revert_block_request) => {
                CommitterResponse::RevertBlock(self.revert_block(revert_block_request).await)
            }
        }
    }
}
