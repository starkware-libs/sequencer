use apollo_committer_types::communication::{CommitterRequest, CommitterResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
#[cfg(feature = "os_input")]
use starknet_committer::db::forest_trait::forest_trait_witnesses::ForestStorageWithWitnesses;
#[cfg(not(feature = "os_input"))]
use starknet_committer::db::forest_trait::ForestStorageWithEmptyReadContext;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::ImmutableReadOnlyStorage;

use crate::committer::{ApolloCommitter, Committer, StorageConstructor};

pub type LocalCommitterServer =
    LocalComponentServer<ApolloCommitter, CommitterRequest, CommitterResponse>;
pub type RemoteCommitterServer = RemoteComponentServer<CommitterRequest, CommitterResponse>;

// `CommitterRequest` without variant `ReadPathsAndCommitBlock` for `os_input` feature.
#[cfg(not(feature = "os_input"))]
#[async_trait]
impl<S: StorageConstructor, ForestDB: ForestStorageWithEmptyReadContext<Storage = S>>
    ComponentRequestHandler<CommitterRequest, CommitterResponse> for Committer<S, ForestDB>
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

#[cfg(feature = "os_input")]
#[async_trait]
impl<S, ForestDB> ComponentRequestHandler<CommitterRequest, CommitterResponse>
    for Committer<S, ForestDB>
where
    S: StorageConstructor + ImmutableReadOnlyStorage + 'static,
    ForestDB: ForestStorageWithWitnesses<Storage = S>,
{
    async fn handle_request(&mut self, request: CommitterRequest) -> CommitterResponse {
        match request {
            CommitterRequest::CommitBlock(commit_block_request) => {
                CommitterResponse::CommitBlock(self.commit_block(commit_block_request).await)
            }
            CommitterRequest::RevertBlock(revert_block_request) => {
                CommitterResponse::RevertBlock(self.revert_block(revert_block_request).await)
            }
            CommitterRequest::ReadPathsAndCommitBlock(req) => {
                CommitterResponse::ReadPathsAndCommitBlock(
                    self.read_paths_and_commit_block(req).await,
                )
            }
        }
    }
}
