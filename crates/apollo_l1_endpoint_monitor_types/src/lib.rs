use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest};
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
    L1_ENDPOINT_MONITOR_LOCAL_MSGS_PROCESSED,
    L1_ENDPOINT_MONITOR_LOCAL_MSGS_RECEIVED,
    L1_ENDPOINT_MONITOR_LOCAL_QUEUE_DEPTH,
    L1_ENDPOINT_MONITOR_REMOTE_MSGS_PROCESSED,
    L1_ENDPOINT_MONITOR_REMOTE_MSGS_RECEIVED,
    L1_ENDPOINT_MONITOR_REMOTE_NUMBER_OF_CONNECTIONS,
    L1_ENDPOINT_MONITOR_REMOTE_VALID_MSGS_RECEIVED,
    L1_ENDPOINT_MONITOR_SEND_ATTEMPTS,
};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_metrics::{define_metrics, generate_permutation_labels};
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
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

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(L1EndpointMonitorRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum L1EndpointMonitorRequest {
    GetActiveL1Endpoint(),
}
impl_debug_for_infra_requests_and_responses!(L1EndpointMonitorRequest);
impl_labeled_request!(L1EndpointMonitorRequest, L1EndpointMonitorRequestLabelValue);
impl PrioritizedRequest for L1EndpointMonitorRequest {}

generate_permutation_labels! {
    L1_ENDPOINT_MONITOR_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, L1EndpointMonitorRequestLabelValue),
}

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

define_metrics!(
    Infra => {
        LabeledMetricHistogram { L1_ENDPOINT_MONITOR_PROCESSING_TIMES_SECS, "l1_endpoint_monitor_processing_times_secs", "Request processing times of the L1 endpoint monitor (secs)", labels = L1_ENDPOINT_MONITOR_REQUEST_LABELS },
        LabeledMetricHistogram { L1_ENDPOINT_MONITOR_QUEUEING_TIMES_SECS, "l1_endpoint_monitor_queueing_times_secs", "Request queueing times of the L1 endpoint monitor (secs)", labels = L1_ENDPOINT_MONITOR_REQUEST_LABELS },
        LabeledMetricHistogram { L1_ENDPOINT_MONITOR_LOCAL_RESPONSE_TIMES_SECS, "l1_endpoint_monitor_local_response_times_secs", "Request local response times of the L1 endpoint monitor (secs)", labels = L1_ENDPOINT_MONITOR_REQUEST_LABELS },
        LabeledMetricHistogram { L1_ENDPOINT_MONITOR_REMOTE_RESPONSE_TIMES_SECS, "l1_endpoint_monitor_remote_response_times_secs", "Request remote response times of the L1 endpoint monitor (secs)", labels = L1_ENDPOINT_MONITOR_REQUEST_LABELS },
        LabeledMetricHistogram { L1_ENDPOINT_MONITOR_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS, "l1_endpoint_monitor_remote_client_communication_failure_times_secs", "Request remote client communication failure times of the L1 endpoint monitor (secs)", labels = L1_ENDPOINT_MONITOR_REQUEST_LABELS },
    },
);

pub const L1_ENDPOINT_MONITOR_INFRA_METRICS: InfraMetrics = InfraMetrics {
    local_client_metrics: LocalClientMetrics::new(&L1_ENDPOINT_MONITOR_LOCAL_RESPONSE_TIMES_SECS),
    remote_client_metrics: RemoteClientMetrics::new(
        &L1_ENDPOINT_MONITOR_SEND_ATTEMPTS,
        &L1_ENDPOINT_MONITOR_REMOTE_RESPONSE_TIMES_SECS,
        &L1_ENDPOINT_MONITOR_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    ),
    local_server_metrics: LocalServerMetrics::new(
        &L1_ENDPOINT_MONITOR_LOCAL_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_LOCAL_MSGS_PROCESSED,
        &L1_ENDPOINT_MONITOR_LOCAL_QUEUE_DEPTH,
        &L1_ENDPOINT_MONITOR_PROCESSING_TIMES_SECS,
        &L1_ENDPOINT_MONITOR_QUEUEING_TIMES_SECS,
    ),
    remote_server_metrics: RemoteServerMetrics::new(
        &L1_ENDPOINT_MONITOR_REMOTE_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_REMOTE_VALID_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_REMOTE_MSGS_PROCESSED,
        &L1_ENDPOINT_MONITOR_REMOTE_NUMBER_OF_CONNECTIONS,
    ),
};
