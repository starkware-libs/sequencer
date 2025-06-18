use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;
use url::Url;

pub type L1EndpointMonitorResult<T> = Result<T, L1EndpointMonitorError>;
pub type L1EndpointMonitorClientResult<T> = Result<T, L1EndpointMonitorClientError>;
pub type SharedL1EndpointMonitorClient = Arc<dyn L1EndpointMonitorClient>;

#[cfg_attr(any(feature = "testing", test), mockall::automock)]
#[async_trait]
pub trait L1EndpointMonitorClient: Send + Sync {
    async fn get_active_l1_endpoint(&self) -> L1EndpointMonitorClientResult<Url>;
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorRequest {
    GetActiveL1Endpoint(),
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EndpointMonitorResponse {
    GetActiveL1Endpoint(L1EndpointMonitorResult<Url>),
}

#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum L1EndpointMonitorError {
    #[error(
        "In order to initialize the L1 endpoint monitor, you must provide at least one L1 \
         endpoint URL in the config."
    )]
    InitializationError,
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
