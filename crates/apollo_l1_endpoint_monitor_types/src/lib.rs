use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentClient;
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;
use url::Url;

pub type L1EndpointMonitorResult<T> = Result<T, L1EndpointMonitorError>;
pub type L1EndpointMonitorClientResult<T> = Result<T, L1EndpointMonitorClientError>;

#[async_trait]
impl<ComponentClientType> L1EndpointMonitorClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<L1EndpointMonitorRequest, L1EndpointMonitorResponse>,
{
    async fn ensure_operational(
        &self,
        url: Url,
    ) -> L1EndpointMonitorClientResult<L1EndpointOperationalStatus> {
        let request = L1EndpointMonitorRequest::EnsureOperational(url);

        handle_all_response_variants!(
            L1EndpointMonitorResponse,
            EnsureOperational,
            L1EndpointMonitorClientError,
            L1EndpointMonitorError,
            Direct
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1EndpointOperationalStatus {
    Operational,
    NonOperational { new_operational_node_url: Url },
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorRequest {
    EnsureOperational(Url),
}
impl_debug_for_infra_requests_and_responses!(L1EndpointMonitorRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorResponse {
    EnsureOperational(L1EndpointMonitorResult<L1EndpointOperationalStatus>),
}
impl_debug_for_infra_requests_and_responses!(L1EndpointMonitorResponse);

#[cfg_attr(any(feature = "testing", test), mockall::automock)]
#[async_trait]
pub trait L1EndpointMonitorClient: Send + Sync {
    async fn ensure_operational(
        &self,
        url: Url,
    ) -> L1EndpointMonitorClientResult<L1EndpointOperationalStatus>;
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum L1EndpointMonitorError {
    #[error("All L1 endpoints are non-operational.")]
    NoActiveL1Endpoint,
}

#[derive(Clone, Debug, Error)]
pub enum L1EndpointMonitorClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1EndpointMonitorError(#[from] L1EndpointMonitorError),
}
