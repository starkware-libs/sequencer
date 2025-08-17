use apollo_commitment_sync_types::communication::{CommitmentSyncRequest, CommitmentSyncResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::commitment_sync::CommitmentSync;

pub type LocalCommitmentSyncServer =
    LocalComponentServer<CommitmentSync, CommitmentSyncRequest, CommitmentSyncResponse>;
pub type RemoteCommitmentSyncServer =
    RemoteComponentServer<CommitmentSyncRequest, CommitmentSyncResponse>;

#[async_trait]
impl ComponentRequestHandler<CommitmentSyncRequest, CommitmentSyncResponse> for CommitmentSync {
    async fn handle_request(&mut self, request: CommitmentSyncRequest) -> CommitmentSyncResponse {
        match request {
            CommitmentSyncRequest::Commit(input) => {
                CommitmentSyncResponse::Commit(self.commit(input))
            }
            CommitmentSyncRequest::GetBlockHash(input) => {
                CommitmentSyncResponse::GetBlockHash(self.get_block_hash(input))
            }
            CommitmentSyncRequest::GetStateCommitment(input) => {
                CommitmentSyncResponse::GetStateCommitment(self.get_state_commitment(input))
            }
        }
    }
}
