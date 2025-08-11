mod concurrent_servers_test;
mod local_component_client_server_test;
mod remote_component_client_server_test;
mod server_metrics_test;

use std::sync::Arc;

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_metrics::metrics::{MetricCounter, MetricGauge, MetricHistogram, MetricScope};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use strum_macros::AsRefStr;
use tokio::sync::Mutex;

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentRequestHandler, ComponentStarter};
use crate::metrics::{LocalServerMetrics, RemoteClientMetrics, RemoteServerMetrics};

pub(crate) type ValueA = Felt;
pub(crate) type ValueB = Felt;
pub(crate) type ResultA = ClientResult<ValueA>;
pub(crate) type ResultB = ClientResult<ValueB>;

// Define mock local server metrics.
const TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "test_msgs_received",
    "test_msgs_received_filter",
    "Test messages received counter",
    0,
);

const TEST_MSGS_PROCESSED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "test_msgs_processed",
    "test_msgs_processed_filter",
    "Test messages processed counter",
    0,
);

const TEST_QUEUE_DEPTH: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "queue_queue_depth",
    "queue_queue_depth_filter",
    "Test channel queue depth gauge",
);

pub(crate) const TEST_LOCAL_SERVER_METRICS: LocalServerMetrics =
    LocalServerMetrics::new(&TEST_MSGS_RECEIVED, &TEST_MSGS_PROCESSED, &TEST_QUEUE_DEPTH);

const REMOTE_TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_test_msgs_received",
    "remote_test_msgs_received_filter",
    "Remote test messages received counter",
    0,
);

const REMOTE_VALID_TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_valid_test_msgs_received",
    "remote_valid_test_msgs_received_filter",
    "Valid remote test messages received counter",
    0,
);

const REMOTE_TEST_MSGS_PROCESSED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_test_msgs_processed",
    "remote_test_msgs_processed_filter",
    "Remote test messages processed counter",
    0,
);

const REMOTE_NUMBER_OF_CONNECTIONS: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "remote_number_of_connections",
    "remote_number_of_connections_filter",
    "Remote number of connections gauge",
);

const EXAMPLE_HISTOGRAM_METRIC: MetricHistogram = MetricHistogram::new(
    MetricScope::Infra,
    "example_histogram_metric",
    "example_histogram_metric_filter",
    "example_histogram_metric_sum_filter",
    "example_histogram_metric_count_filter",
    "Example histogram metrics",
);

pub(crate) const TEST_REMOTE_SERVER_METRICS: RemoteServerMetrics = RemoteServerMetrics::new(
    &REMOTE_TEST_MSGS_RECEIVED,
    &REMOTE_VALID_TEST_MSGS_RECEIVED,
    &REMOTE_TEST_MSGS_PROCESSED,
    &REMOTE_NUMBER_OF_CONNECTIONS,
);

pub(crate) const TEST_REMOTE_CLIENT_METRICS: RemoteClientMetrics =
    RemoteClientMetrics::new(&EXAMPLE_HISTOGRAM_METRIC);

// Define the shared fixture
pub static AVAILABLE_PORTS: Lazy<Arc<Mutex<AvailablePorts>>> = Lazy::new(|| {
    let available_ports = AvailablePorts::new(TestIdentifier::InfraUnitTests.into(), 0);
    Arc::new(Mutex::new(available_ports))
});

#[derive(Serialize, Deserialize, Debug, AsRefStr)]
pub enum ComponentARequest {
    AGetValue,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentAResponse {
    AGetValue(ValueA),
}

#[derive(Serialize, Deserialize, Debug, AsRefStr)]
pub enum ComponentBRequest {
    BGetValue,
    BSetValue(ValueB),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentBResponse {
    BGetValue(ValueB),
    BSetValue,
}

#[async_trait]
pub(crate) trait ComponentAClientTrait: Send + Sync {
    async fn a_get_value(&self) -> ResultA;
}

#[async_trait]
pub(crate) trait ComponentBClientTrait: Send + Sync {
    async fn b_get_value(&self) -> ResultB;
    async fn b_set_value(&self, value: ValueB) -> ClientResult<()>;
}

pub(crate) struct ComponentA {
    b: Box<dyn ComponentBClientTrait>,
}

impl ComponentA {
    pub fn new(b: Box<dyn ComponentBClientTrait>) -> Self {
        Self { b }
    }

    pub async fn a_get_value(&self) -> ValueA {
        self.b.b_get_value().await.unwrap()
    }
}

impl ComponentStarter for ComponentA {}

pub(crate) struct ComponentB {
    value: ValueB,
    _a: Box<dyn ComponentAClientTrait>,
}

impl ComponentB {
    pub fn new(value: ValueB, a: Box<dyn ComponentAClientTrait>) -> Self {
        Self { value, _a: a }
    }

    pub fn b_get_value(&self) -> ValueB {
        self.value
    }

    pub fn b_set_value(&mut self, value: ValueB) {
        self.value = value;
    }
}

impl ComponentStarter for ComponentB {}

pub(crate) async fn test_a_b_functionality(
    a_client: impl ComponentAClientTrait,
    b_client: impl ComponentBClientTrait,
    expected_value: ValueA,
) {
    // Check the setup value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), expected_value);

    let new_expected_value: ValueA = expected_value + 1;
    // Check that setting a new value to component B succeeds.
    assert!(b_client.b_set_value(new_expected_value).await.is_ok());
    // Check the new value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), new_expected_value);
}

#[async_trait]
impl ComponentRequestHandler<ComponentARequest, ComponentAResponse> for ComponentA {
    async fn handle_request(&mut self, request: ComponentARequest) -> ComponentAResponse {
        match request {
            ComponentARequest::AGetValue => ComponentAResponse::AGetValue(self.a_get_value().await),
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<ComponentBRequest, ComponentBResponse> for ComponentB {
    async fn handle_request(&mut self, request: ComponentBRequest) -> ComponentBResponse {
        match request {
            ComponentBRequest::BGetValue => ComponentBResponse::BGetValue(self.b_get_value()),
            ComponentBRequest::BSetValue(value) => {
                self.b_set_value(value);
                ComponentBResponse::BSetValue
            }
        }
    }
}
