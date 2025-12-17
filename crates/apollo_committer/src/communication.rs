use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use starknet_patricia_storage::storage_trait::Storage;

use crate::committer::Committer;

pub type LocalCommitterServer<S> =
    LocalComponentServer<Committer<S>, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

#[async_trait]
impl<S: Storage + Default> ComponentRequestHandler<CommitterRequest, CommitterResponse>
    for Committer<S>
{
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::CommitBlock(commit_block_request) => {
                CommitterResponse::CommitBlock(self.commit_block(commit_block_request).await)
            }
            CommitterRequest::RevertBlock(_) => {
                // TODO(Yoav): Call the committer.
                unimplemented!()
            }
        }
    }
}
