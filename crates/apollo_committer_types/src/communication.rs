use std::sync::Arc;

use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{PrioritizedRequest, RequestWrapper};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::committer_types::{CommitBlockInput, CommitBlockResponse};
use crate::errors::{CommitterClientResult, CommitterResult};

#[async_trait]
#[cfg_attr(any(feature = "testing", test), mockall::automock)]
pub trait CommitterClient: Send + Sync {
    /// Applies the state diff on the state trees and computes the new state roots.
    async fn commit_block(
        &self,
        input: CommitBlockInput,
    ) -> CommitterClientResult<CommitBlockResponse>;
}

pub type SharedCommitterClient = Arc<dyn CommitterClient>;

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(CommitterRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum CommitterRequest {
    CommitBlock(CommitBlockInput),
}

impl_debug_for_infra_requests_and_responses!(CommitterRequest);
impl_labeled_request!(CommitterRequest, CommitterRequestLabelValue);
impl PrioritizedRequest for CommitterRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum CommitterResponse {
    CommitBlock(CommitterResult<CommitBlockResponse>),
}

impl_debug_for_infra_requests_and_responses!(CommitterResponse);

pub type LocalCommitterClient = LocalComponentClient<CommitterRequest, CommitterResponse>;
pub type RemoteCommitterClient = RemoteComponentClient<CommitterRequest, CommitterResponse>;
pub type CommitterRequestWrapper = RequestWrapper<CommitterRequest, CommitterResponse>;

generate_permutation_labels! {
    COMMITTER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, CommitterRequestLabelValue),
}
