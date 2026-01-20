use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use starknet_committer::block_committer::commit::CommitBlockTrait;
use starknet_committer::db::forest_trait::ForestStorage;

use crate::committer::{ApolloCommitter, Committer};

pub type LocalCommitterServer =
    LocalComponentServer<ApolloCommitter, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl<ForestDB: ForestStorage, BlockCommitter: CommitBlockTrait>
    ComponentRequestHandler<CommitterRequest, CommitterResponse>
    for Committer<ForestDB, BlockCommitter>
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
