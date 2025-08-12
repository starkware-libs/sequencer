use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;
use url::Url;

pub type L1EndpointMonitorResult<T> = Result<T, L1EndpointMonitorError>;
pub type L1EndpointMonitorClientResult<T> = Result<T, L1EndpointMonitorClientError>;
pub type SharedL1EndpointMonitorClient = Arc<dyn L1EndpointMonitorClient>;

#[async_trait]
impl<ComponentClientType> L1EndpointMonitorClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<L1EndpointMonitorRequest, L1EndpointMonitorResponse>,
{
    async fn get_active_l1_endpoint(&self) -> L1EndpointMonitorClientResult<Url> {
        let request = L1EndpointMonitorRequest::GetActiveL1Endpoint();

        handle_all_response_variants!(
            L1EndpointMonitorResponse,
            GetActiveL1Endpoint,
            L1EndpointMonitorClientError,
            L1EndpointMonitorError,
            Direct
        )
    }
}
#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorRequest {
    GetActiveL1Endpoint(),
}
impl_debug_for_infra_requests_and_responses!(L1EndpointMonitorRequest);
impl PrioritizedRequest for L1EndpointMonitorRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorResponse {
    GetActiveL1Endpoint(L1EndpointMonitorResult<Url>),
}
impl_debug_for_infra_requests_and_responses!(L1EndpointMonitorResponse);

#[cfg_attr(any(feature = "testing", test), mockall::automock)]
#[async_trait]
pub trait L1EndpointMonitorClient: Send + Sync {
    async fn get_active_l1_endpoint(&self) -> L1EndpointMonitorClientResult<Url>;
}

#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum L1EndpointMonitorError {
    #[error("Unknown initial L1 endpoint URL not present in the config: {unknown_url}")]
    InitializationError { unknown_url: Url },
    #[error("All L1 endpoints are non-operational")]
    NoActiveL1Endpoint,
}

#[derive(Clone, Debug, Error)]
pub enum L1EndpointMonitorClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1EndpointMonitorError(#[from] L1EndpointMonitorError),
}
