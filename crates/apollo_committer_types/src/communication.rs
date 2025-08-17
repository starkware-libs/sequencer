use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::committer_types::{StateCommitment, StateCommitmentInput};
use crate::errors::{CommitterError, CommitterResult};
pub type LocalCommitterClient = LocalComponentClient<CommitterRequest, CommitterResponse>;
pub type RemoteCommitterClient = RemoteComponentClient<CommitterRequest, CommitterResponse>;
pub type CommitterClientResult<T> = Result<T, CommitterClientError>;
pub type CommitterRequestAndResponseSender =
    ComponentRequestAndResponseSender<CommitterRequest, CommitterResponse>;
pub type SharedCommitterClient = Arc<dyn CommitterClient>;

/// Client trait for communicating with the committer component.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait CommitterClient: Send + Sync {
    /// Commits the state diff on the given state.
    /// Returns the new state roots.
    async fn commit(&self, input: StateCommitmentInput) -> CommitterClientResult<StateCommitment>;
}

/// Requests that can be sent to the committer component.
#[derive(Serialize, Deserialize, Clone, AsRefStr)]
pub enum CommitterRequest {
    Commit(StateCommitmentInput),
}

/// Responses from the committer component.
#[derive(Serialize, Deserialize, AsRefStr)]
pub enum CommitterResponse {
    Commit(CommitterResult<StateCommitment>),
}

impl_debug_for_infra_requests_and_responses!(CommitterResponse);
impl_debug_for_infra_requests_and_responses!(CommitterRequest);

#[derive(Debug, Error)]
pub enum CommitterClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    Committer(#[from] CommitterError),
}

#[async_trait]
impl<ComponentClientType> CommitterClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<CommitterRequest, CommitterResponse>,
{
    async fn commit(&self, input: StateCommitmentInput) -> CommitterClientResult<StateCommitment> {
        let request = CommitterRequest::Commit(input);
        handle_all_response_variants!(
            CommitterResponse,
            Commit,
            CommitterClientError,
            Committer,
            Direct
        )
    }
}
