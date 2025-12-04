use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricGauge,
    MetricHistogram,
    COLLECT_SEQUENCER_PROFILING_METRICS,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::{Deserialize, Serialize};

use crate::requests::LABEL_NAME_REQUEST_VARIANT;
use crate::tokio_metrics::setup_tokio_metrics;

pub const HISTOGRAM_BUCKETS: &[f64] = &[
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0,
    100.0, 250.0,
];

/// Configuration for metrics collection.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MetricsConfig {
    /// Whether to collect metrics at all.
    pub collect_metrics: bool,
    /// Whether to collect profiling metrics.
    pub collect_profiling_metrics: bool,
}

impl MetricsConfig {
    /// Returns a config with all metrics collection enabled.
    pub const fn enabled() -> Self {
        Self { collect_metrics: true, collect_profiling_metrics: true }
    }

    /// Returns a config with all metrics collection disabled.
    pub const fn disabled() -> Self {
        Self { collect_metrics: false, collect_profiling_metrics: false }
    }
}

/// Initializes the metrics recorder and tokio metrics if metrics collection is enabled.
/// This should be called once during application startup, before creating components that use
/// metrics.
///
/// Returns a PrometheusHandle if metrics collection is enabled, None otherwise.
///
/// # Example
/// ```no_run
/// use apollo_infra::metrics::{initialize_metrics_recorder, MetricsConfig};
///
/// let config = MetricsConfig::enabled();
/// let prometheus_handle = initialize_metrics_recorder(config);
/// ```
pub fn initialize_metrics_recorder(config: MetricsConfig) -> Option<PrometheusHandle> {
    // TODO(Tsabary): consider error handling
    let prometheus_handle = if config.collect_metrics {
        // TODO(Lev): add tests that show the metrics are collected / not collected based on the
        // config value.
        COLLECT_SEQUENCER_PROFILING_METRICS
            .set(config.collect_profiling_metrics)
            .expect("Should be able to set profiling metrics collection.");

        Some(
            PrometheusBuilder::new()
                .set_buckets(HISTOGRAM_BUCKETS)
                .expect("Should be able to set buckets")
                .install_recorder()
                .expect("should be able to build the recorder and install it globally"),
        )
    } else {
        None
    };

    // Setup tokio metrics along with other metrics initialization
    setup_tokio_metrics();

    prometheus_handle
}

/// Metrics of a local client.
#[derive(Clone)]
pub struct LocalClientMetrics {
    response_times: &'static LabeledMetricHistogram,
}

impl LocalClientMetrics {
    pub const fn new(response_times: &'static LabeledMetricHistogram) -> Self {
        Self { response_times }
    }
    pub fn register(&self) {
        self.response_times.register();
    }

    pub fn record_response_time(&self, duration_secs: f64, request_label: &'static str) {
        self.response_times.record(duration_secs, &[(LABEL_NAME_REQUEST_VARIANT, request_label)]);
    }

    pub fn get_response_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.response_times
    }

    pub fn get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
        vec![self.response_times]
    }
}

/// Metrics of a remote client.
#[derive(Clone)]
pub struct RemoteClientMetrics {
    /// Histogram to track the number of send attempts to a remote server.
    attempts: &'static MetricHistogram,
    response_times: &'static LabeledMetricHistogram,
    communication_failure_times: &'static LabeledMetricHistogram,
}

impl RemoteClientMetrics {
    pub const fn new(
        attempts: &'static MetricHistogram,
        response_times: &'static LabeledMetricHistogram,
        communication_failure_times: &'static LabeledMetricHistogram,
    ) -> Self {
        Self { attempts, response_times, communication_failure_times }
    }

    pub fn register(&self) {
        self.attempts.register();
        self.response_times.register();
        self.communication_failure_times.register();
    }

    pub fn record_attempt(&self, value: usize) {
        self.attempts.record_lossy(value);
    }

    pub fn record_response_time(&self, duration_secs: f64, request_label: &'static str) {
        self.response_times.record(duration_secs, &[(LABEL_NAME_REQUEST_VARIANT, request_label)]);
    }

    pub fn record_communication_failure(&self, duration_secs: f64, request_label: &'static str) {
        self.communication_failure_times
            .record(duration_secs, &[(LABEL_NAME_REQUEST_VARIANT, request_label)]);
    }

    // TODO(Tsabary): consider deleting
    pub fn get_attempts_metric(&self) -> &'static MetricHistogram {
        self.attempts
    }

    pub fn get_response_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.response_times
    }

    pub fn get_communication_failure_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.communication_failure_times
    }

    pub fn get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
        vec![self.response_times, self.communication_failure_times]
    }
}

/// A struct to contain all metrics for a local server.
pub struct LocalServerMetrics {
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    high_priority_queue_depth: &'static MetricGauge,
    normal_priority_queue_depth: &'static MetricGauge,
    processing_times: &'static LabeledMetricHistogram,
    queueing_times: &'static LabeledMetricHistogram,
}

impl LocalServerMetrics {
    pub const fn new(
        received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        high_priority_queue_depth: &'static MetricGauge,
        normal_priority_queue_depth: &'static MetricGauge,
        processing_times: &'static LabeledMetricHistogram,
        queueing_times: &'static LabeledMetricHistogram,
    ) -> Self {
        Self {
            received_msgs,
            processed_msgs,
            high_priority_queue_depth,
            normal_priority_queue_depth,
            processing_times,
            queueing_times,
        }
    }

    pub fn register(&self) {
        self.received_msgs.register();
        self.processed_msgs.register();
        self.high_priority_queue_depth.register();
        self.normal_priority_queue_depth.register();
        self.processing_times.register();
        self.queueing_times.register();
    }

    pub fn increment_received(&self) {
        self.received_msgs.increment(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_received_value(&self, metrics_as_string: &str) -> u64 {
        self.received_msgs
            .parse_numeric_metric::<u64>(metrics_as_string)
            .expect("received_msgs metrics should be available")
    }

    pub fn increment_processed(&self) {
        self.processed_msgs.increment(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_processed_value(&self, metrics_as_string: &str) -> u64 {
        self.processed_msgs
            .parse_numeric_metric::<u64>(metrics_as_string)
            .expect("processed_msgs metrics should be available")
    }

    pub fn set_high_priority_queue_depth(&self, value: usize) {
        self.high_priority_queue_depth.set_lossy(value);
    }

    pub fn set_normal_priority_queue_depth(&self, value: usize) {
        self.normal_priority_queue_depth.set_lossy(value);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_high_priority_queue_depth_value(&self, metrics_as_string: &str) -> usize {
        self.high_priority_queue_depth
            .parse_numeric_metric::<usize>(metrics_as_string)
            .expect("high_priority_queue_depth metrics should be available")
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_normal_priority_queue_depth_value(&self, metrics_as_string: &str) -> usize {
        self.normal_priority_queue_depth
            .parse_numeric_metric::<usize>(metrics_as_string)
            .expect("normal_priority_queue_depth metrics should be available")
    }

    // TODO(Tsabary): add the getter fns for tests.
    pub fn record_processing_time(&self, duration_secs: f64, request_label: &'static str) {
        self.processing_times.record(duration_secs, &[(LABEL_NAME_REQUEST_VARIANT, request_label)]);
    }

    pub fn record_queueing_time(&self, duration_secs: f64, request_label: &'static str) {
        self.queueing_times.record(duration_secs, &[(LABEL_NAME_REQUEST_VARIANT, request_label)]);
    }

    pub fn get_processing_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.processing_times
    }

    pub fn get_queueing_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.queueing_times
    }

    pub fn get_received_metric(&self) -> &'static MetricCounter {
        self.received_msgs
    }

    // TODO(Tsabary): consider deleting
    pub fn get_processed_metric(&self) -> &'static MetricCounter {
        self.processed_msgs
    }

    pub fn get_high_priority_queue_depth_metric(&self) -> &'static MetricGauge {
        self.high_priority_queue_depth
    }

    pub fn get_normal_priority_queue_depth_metric(&self) -> &'static MetricGauge {
        self.normal_priority_queue_depth
    }

    pub fn get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
        vec![self.processing_times, self.queueing_times]
    }
}

/// A struct to contain all metrics for a remote server.
pub struct RemoteServerMetrics {
    total_received_msgs: &'static MetricCounter,
    valid_received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    number_of_connections: &'static MetricGauge,
}

impl RemoteServerMetrics {
    pub const fn new(
        total_received_msgs: &'static MetricCounter,
        valid_received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        number_of_connections: &'static MetricGauge,
    ) -> Self {
        Self { total_received_msgs, valid_received_msgs, processed_msgs, number_of_connections }
    }

    pub fn register(&self) {
        self.total_received_msgs.register();
        self.valid_received_msgs.register();
        self.processed_msgs.register();
        self.number_of_connections.register();
    }

    pub fn increment_total_received(&self) {
        self.total_received_msgs.increment(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_total_received_value(&self, metrics_as_string: &str) -> u64 {
        self.total_received_msgs
            .parse_numeric_metric::<u64>(metrics_as_string)
            .expect("total_received_msgs metrics should be available")
    }

    pub fn increment_valid_received(&self) {
        self.valid_received_msgs.increment(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_valid_received_value(&self, metrics_as_string: &str) -> u64 {
        self.valid_received_msgs
            .parse_numeric_metric::<u64>(metrics_as_string)
            .expect("valid_received_msgs metrics should be available")
    }

    pub fn increment_processed(&self) {
        self.processed_msgs.increment(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_processed_value(&self, metrics_as_string: &str) -> u64 {
        self.processed_msgs
            .parse_numeric_metric::<u64>(metrics_as_string)
            .expect("processed_msgs metrics should be available")
    }

    pub fn increment_number_of_connections(&self) {
        self.number_of_connections.increment(1);
    }

    pub fn decrement_number_of_connections(&self) {
        self.number_of_connections.decrement(1);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_number_of_connections_value(&self, metrics_as_string: &str) -> usize {
        self.number_of_connections
            .parse_numeric_metric::<usize>(metrics_as_string)
            .expect("number_of_connections metrics should be available")
    }

    pub fn get_total_received_metric(&self) -> &'static MetricCounter {
        self.total_received_msgs
    }

    // TODO(Tsabary): consider deleting
    pub fn get_valid_received_metric(&self) -> &'static MetricCounter {
        self.valid_received_msgs
    }

    // TODO(Tsabary): consider deleting
    pub fn get_processed_metric(&self) -> &'static MetricCounter {
        self.processed_msgs
    }

    pub fn get_number_of_connections_metric(&self) -> &'static MetricGauge {
        self.number_of_connections
    }

    pub fn get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
        vec![]
    }
}

pub struct InfraMetrics {
    local_client_metrics: LocalClientMetrics,
    remote_client_metrics: RemoteClientMetrics,
    local_server_metrics: LocalServerMetrics,
    remote_server_metrics: RemoteServerMetrics,
}

impl InfraMetrics {
    pub const fn new(
        local_client_metrics: LocalClientMetrics,
        remote_client_metrics: RemoteClientMetrics,
        local_server_metrics: LocalServerMetrics,
        remote_server_metrics: RemoteServerMetrics,
    ) -> Self {
        Self {
            local_client_metrics,
            remote_client_metrics,
            local_server_metrics,
            remote_server_metrics,
        }
    }

    pub fn get_local_client_metrics(&self) -> &LocalClientMetrics {
        &self.local_client_metrics
    }

    pub fn get_remote_client_metrics(&self) -> &RemoteClientMetrics {
        &self.remote_client_metrics
    }

    pub fn get_local_server_metrics(&self) -> &LocalServerMetrics {
        &self.local_server_metrics
    }

    pub fn get_remote_server_metrics(&self) -> &RemoteServerMetrics {
        &self.remote_server_metrics
    }
}
