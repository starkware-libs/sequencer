use std::sync::Arc;

use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{
    handle_all_response_variants,
    impl_debug_for_infra_requests_and_responses,
    impl_labeled_request,
};
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, IntoStaticStr, VariantNames};

use crate::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
#[cfg(feature = "os_input")]
use crate::committer_types::{ReadPathsAndCommitBlockRequest, ReadPathsAndCommitBlockResponse};
use crate::errors::{CommitterClientError, CommitterClientResult, CommitterResult};

pub type LocalCommitterClient = LocalComponentClient<CommitterRequest, CommitterResponse>;
pub type RemoteCommitterClient = RemoteComponentClient<CommitterRequest, CommitterResponse>;
pub type CommitterRequestWrapper = RequestWrapper<CommitterRequest, CommitterResponse>;

pub type SharedCommitterClient = Arc<dyn CommitterClient>;

#[async_trait]
#[cfg_attr(any(feature = "testing", test), automock)]
pub trait CommitterClient: Send + Sync {
    /// Applies the state diff on the state trees and computes the new state root.
    async fn commit_block(
        &self,
        input: CommitBlockRequest,
    ) -> CommitterClientResult<CommitBlockResponse>;

    /// Applies the reversed state diff on the state trees and computes the previous state root.
    async fn revert_block(
        &self,
        input: RevertBlockRequest,
    ) -> CommitterClientResult<RevertBlockResponse>;

    #[cfg(feature = "os_input")]
    /// Applies the state diff, collects merged Patricia witnesses for OS input, and persists replay
    /// data (digest + payload).
    async fn read_paths_and_commit_block(
        &self,
        input: ReadPathsAndCommitBlockRequest,
    ) -> CommitterClientResult<ReadPathsAndCommitBlockResponse>;
}

#[derive(Serialize, Deserialize, Clone, AsRefStr)]
pub enum CommitterRequest {
    CommitBlock(CommitBlockRequest),
    RevertBlock(RevertBlockRequest),
    #[cfg(feature = "os_input")]
    ReadPathsAndCommitBlock(ReadPathsAndCommitBlockRequest),
}

/// Payload-free discriminants of [`CommitterRequest`], used as a metric label. Independent of
/// `os_input` on purpose: the label set is identical in every build, while the request variant and
/// its payload stay feature-gated. Hand-written rather than derived via `EnumDiscriminants` so the
/// `os_input`-only request variant does not gate the label out — otherwise every consumer would
/// have to re-gate on its own `os_input`, which diverges across crates under `--all-features`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr, EnumIter, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum CommitterRequestLabelValue {
    CommitBlock,
    RevertBlock,
    ReadPathsAndCommitBlock,
}

impl From<&CommitterRequest> for CommitterRequestLabelValue {
    fn from(request: &CommitterRequest) -> Self {
        match request {
            CommitterRequest::CommitBlock(_) => Self::CommitBlock,
            CommitterRequest::RevertBlock(_) => Self::RevertBlock,
            #[cfg(feature = "os_input")]
            CommitterRequest::ReadPathsAndCommitBlock(_) => Self::ReadPathsAndCommitBlock,
        }
    }
}

impl_debug_for_infra_requests_and_responses!(CommitterRequest);
impl_labeled_request!(CommitterRequest, CommitterRequestLabelValue);
impl PrioritizedRequest for CommitterRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum CommitterResponse {
    CommitBlock(CommitterResult<CommitBlockResponse>),
    RevertBlock(CommitterResult<RevertBlockResponse>),
    #[cfg(feature = "os_input")]
    ReadPathsAndCommitBlock(CommitterResult<ReadPathsAndCommitBlockResponse>),
}

impl_debug_for_infra_requests_and_responses!(CommitterResponse);

generate_permutation_labels! {
    COMMITTER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, CommitterRequestLabelValue),
}

#[async_trait]
impl<ComponentClientType> CommitterClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<CommitterRequest, CommitterResponse>,
{
    async fn commit_block(
        &self,
        input: CommitBlockRequest,
    ) -> CommitterClientResult<CommitBlockResponse> {
        let request = CommitterRequest::CommitBlock(input);
        handle_all_response_variants!(
            self,
            request,
            CommitterResponse,
            CommitBlock,
            CommitterClientError,
            CommitterError,
            Direct
        )
    }

    async fn revert_block(
        &self,
        input: RevertBlockRequest,
    ) -> CommitterClientResult<RevertBlockResponse> {
        let request = CommitterRequest::RevertBlock(input);
        handle_all_response_variants!(
            self,
            request,
            CommitterResponse,
            RevertBlock,
            CommitterClientError,
            CommitterError,
            Direct
        )
    }

    #[cfg(feature = "os_input")]
    async fn read_paths_and_commit_block(
        &self,
        input: ReadPathsAndCommitBlockRequest,
    ) -> CommitterClientResult<ReadPathsAndCommitBlockResponse> {
        let request = CommitterRequest::ReadPathsAndCommitBlock(input);
        handle_all_response_variants!(
            self,
            request,
            CommitterResponse,
            ReadPathsAndCommitBlock,
            CommitterClientError,
            CommitterError,
            Direct
        )
    }
}
