use std::sync::Arc;

use apollo_committer_types::committer_types::StateCommitment;
use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::errors::{CommitmentSyncError, CommitmentSyncResult};
use crate::types::CommitmentInput;
pub type LocalCommitmentSyncClient =
    LocalComponentClient<CommitmentSyncRequest, CommitmentSyncResponse>;
pub type RemoteCommitmentSyncClient =
    RemoteComponentClient<CommitmentSyncRequest, CommitmentSyncResponse>;
pub type CommitmentSyncClientResult<T> = Result<T, CommitmentSyncClientError>;
pub type CommitmentSyncRequestAndResponseSender =
    ComponentRequestAndResponseSender<CommitmentSyncRequest, CommitmentSyncResponse>;
pub type SharedCommitmentSyncClient = Arc<dyn CommitmentSyncClient>;
pub type GetBlockHashInput = BlockNumber;
pub type GetBlockHashOutput = Option<BlockHash>;
pub type GetStateCommitmentInput = BlockNumber;
pub type GetStateCommitmentOutput = Option<StateCommitment>;

/// Client trait for communicating with the commitment sync component.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait CommitmentSyncClient: Send + Sync {
    /// Commit the state by calculating block hash and state commitment.
    async fn commit(&self, input: CommitmentInput) -> CommitmentSyncClientResult<()>;

    /// Get the block hash for a given block number.
    async fn get_block_hash(
        &self,
        input: GetBlockHashInput, // type GetBlockHashInput = BlockNumber;
    ) -> CommitmentSyncClientResult<GetBlockHashOutput>;

    /// Get the state commitment for a given block number.
    async fn get_state_commitment(
        &self,
        input: GetStateCommitmentInput, // type GetStateCommitmentInput = BlockNumber;
    ) -> CommitmentSyncClientResult<GetStateCommitmentOutput>;
}

/// Requests that can be sent to the commitment sync component.
#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
pub enum CommitmentSyncRequest {
    Commit(CommitmentInput),
    GetBlockHash(GetBlockHashInput),
    GetStateCommitment(GetStateCommitmentInput),
}

/// Responses from the commitment sync component.
#[derive(Serialize, Deserialize, AsRefStr)]
pub enum CommitmentSyncResponse {
    Commit(CommitmentSyncResult<()>),
    GetBlockHash(CommitmentSyncResult<GetBlockHashOutput>),
    GetStateCommitment(CommitmentSyncResult<GetStateCommitmentOutput>),
}

impl_debug_for_infra_requests_and_responses!(CommitmentSyncResponse);

#[derive(Debug, Error)]
pub enum CommitmentSyncClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    CommitmentSync(#[from] CommitmentSyncError),
}

#[async_trait]
impl<ComponentClientType> CommitmentSyncClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<CommitmentSyncRequest, CommitmentSyncResponse>,
{
    async fn commit(&self, input: CommitmentInput) -> CommitmentSyncClientResult<()> {
        let request = CommitmentSyncRequest::Commit(input);
        handle_all_response_variants!(
            CommitmentSyncResponse,
            Commit,
            CommitmentSyncClientError,
            CommitmentSync,
            Direct
        )
    }

    async fn get_block_hash(
        &self,
        input: GetBlockHashInput,
    ) -> CommitmentSyncClientResult<GetBlockHashOutput> {
        let request = CommitmentSyncRequest::GetBlockHash(input);
        handle_all_response_variants!(
            CommitmentSyncResponse,
            GetBlockHash,
            CommitmentSyncClientError,
            CommitmentSync,
            Direct
        )
    }

    async fn get_state_commitment(
        &self,
        input: GetStateCommitmentInput,
    ) -> CommitmentSyncClientResult<GetStateCommitmentOutput> {
        let request = CommitmentSyncRequest::GetStateCommitment(input);
        handle_all_response_variants!(
            CommitmentSyncResponse,
            GetStateCommitment,
            CommitmentSyncClientError,
            CommitmentSync,
            Direct
        )
    }
}
