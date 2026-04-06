use std::net::IpAddr;

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricGauge,
    MetricHistogram,
    MetricScope,
};

use crate::component_server::RemoteServerConfig;
use crate::metrics::{
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use crate::tests::component_a_b_fixture::{COMPONENT_A_REQUEST_LABELS, COMPONENT_B_REQUEST_LABELS};

pub(crate) mod component_a_b_fixture;
mod concurrent_servers;
mod local_component_client_server;
mod local_request_prioritization;
mod remote_component_client_server;
mod remote_server_timeouts;
mod server_metrics;
mod test_utils;

// Define mock local server metrics.
const TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "test_msgs_received",
    "Test messages received counter",
    0,
);

const TEST_MSGS_PROCESSED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "test_msgs_processed",
    "Test messages processed counter",
    0,
);

const TEST_HIGH_PRIORITY_QUEUE_DEPTH: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "high_priority_queue_depth",
    "Test high priority queue depth gauge",
);

const TEST_NORMAL_PRIORITY_QUEUE_DEPTH: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "normal_priority_queue_depth",
    "Test normal priority queue depth gauge",
);

const _TEST_PROCESSING_TIMES_SECS: MetricHistogram =
    MetricHistogram::new(MetricScope::Infra, "processing_times", "Test processing time histogram");

const TEST_PROCESSING_TIMES_SECS_LABELLED_A: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_processing_times_a",
    "Test processing time histogram for component A",
    COMPONENT_A_REQUEST_LABELS,
);

const _TEST_PROCESSING_TIMES_SECS_LABELLED_B: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_processing_times_b",
    "Test processing time histogram for component B",
    COMPONENT_B_REQUEST_LABELS,
);

const _TEST_QUEUEING_TIMES_SECS: MetricHistogram =
    MetricHistogram::new(MetricScope::Infra, "queueing_times", "Test queueing time histogram");

const TEST_QUEUEING_TIMES_SECS_LABELLED_A: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_queueing_times_a",
    "Test queueing time histogram for component A",
    COMPONENT_A_REQUEST_LABELS,
);

const _TEST_QUEUEING_TIMES_SECS_LABELLED_B: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "labeled_queueing_times_b",
    "Test queueing time histogram for component B",
    COMPONENT_B_REQUEST_LABELS,
);

// TODO(alonl): Fix only using component A metrics.
pub(crate) const TEST_LOCAL_SERVER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
    &TEST_MSGS_RECEIVED,
    &TEST_MSGS_PROCESSED,
    &TEST_HIGH_PRIORITY_QUEUE_DEPTH,
    &TEST_NORMAL_PRIORITY_QUEUE_DEPTH,
    &TEST_PROCESSING_TIMES_SECS_LABELLED_A,
    &TEST_QUEUEING_TIMES_SECS_LABELLED_A,
);

const REMOTE_TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_test_msgs_received",
    "Remote test messages received counter",
    0,
);

const REMOTE_VALID_TEST_MSGS_RECEIVED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_valid_test_msgs_received",
    "Valid remote test messages received counter",
    0,
);

const REMOTE_TEST_MSGS_PROCESSED: MetricCounter = MetricCounter::new(
    MetricScope::Infra,
    "remote_test_msgs_processed",
    "Remote test messages processed counter",
    0,
);

const REMOTE_NUMBER_OF_CONNECTIONS: MetricGauge = MetricGauge::new(
    MetricScope::Infra,
    "remote_number_of_connections",
    "Remote number of connections gauge",
);

const EXAMPLE_HISTOGRAM_METRIC: MetricHistogram = MetricHistogram::new(
    MetricScope::Infra,
    "example_histogram_metric",
    "Example histogram metrics",
);

pub(crate) const TEST_REMOTE_SERVER_METRICS: RemoteServerMetrics = RemoteServerMetrics::new(
    &REMOTE_TEST_MSGS_RECEIVED,
    &REMOTE_VALID_TEST_MSGS_RECEIVED,
    &REMOTE_TEST_MSGS_PROCESSED,
    &REMOTE_NUMBER_OF_CONNECTIONS,
);

pub(crate) const TEST_REMOTE_CLIENT_RESPONSE_TIMES: LabeledMetricHistogram =
    LabeledMetricHistogram::new(
        MetricScope::Infra,
        "test_remote_client_response_times",
        "Test remote client response times histogram",
        COMPONENT_A_REQUEST_LABELS,
    );

pub(crate) const TEST_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES: LabeledMetricHistogram =
    LabeledMetricHistogram::new(
        MetricScope::Infra,
        "test_remote_client_communication_failure_times",
        "Test remote client communication failure times histogram",
        COMPONENT_A_REQUEST_LABELS,
    );

pub(crate) const TEST_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &EXAMPLE_HISTOGRAM_METRIC,
    &TEST_REMOTE_CLIENT_RESPONSE_TIMES,
    &TEST_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES,
);

const TEST_LOCAL_CLIENT_RESPONSE_TIMES: LabeledMetricHistogram = LabeledMetricHistogram::new(
    MetricScope::Infra,
    "test_local_client_response_times",
    "Test local client response times histogram",
    COMPONENT_A_REQUEST_LABELS,
);

pub(crate) const TEST_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&TEST_LOCAL_CLIENT_RESPONSE_TIMES);

// Creates an `AvailablePorts` instance with a unique `instance_index`.
// Each test that binds ports should use a different instance_index to get disjoint port ranges.
// This is necessary to allow running tests concurrently in different processes, which do not have a
// shared memory.
pub(crate) fn available_ports_factory(instance_index: u16) -> AvailablePorts {
    AvailablePorts::new(TestIdentifier::InfraUnitTests.into(), instance_index)
}

pub(crate) fn dummy_remote_server_config(ip: IpAddr) -> RemoteServerConfig {
    RemoteServerConfig {
        bind_ip: ip,
        // arbitrary value
        max_streams_per_connection: 5,
        set_tcp_nodelay: true,
        ..Default::default()
    }
}
