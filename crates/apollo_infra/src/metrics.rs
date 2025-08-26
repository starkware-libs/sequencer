use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricGauge,
    MetricHistogram,
};

use crate::requests::LABEL_NAME_REQUEST_VARIANT;

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

    pub fn get_attempts_metric(&self) -> &'static MetricHistogram {
        self.attempts
    }

    pub fn get_response_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.response_times
    }

    pub fn get_communication_failure_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.communication_failure_times
    }
}

/// A struct to contain all metrics for a local server.
pub struct LocalServerMetrics {
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
    processing_times: &'static LabeledMetricHistogram,
    queueing_times: &'static LabeledMetricHistogram,
}

impl LocalServerMetrics {
    pub const fn new(
        received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        queue_depth: &'static MetricGauge,
        processing_times: &'static LabeledMetricHistogram,
        queueing_times: &'static LabeledMetricHistogram,
    ) -> Self {
        Self { received_msgs, processed_msgs, queue_depth, processing_times, queueing_times }
    }

    pub fn register(&self) {
        self.received_msgs.register();
        self.processed_msgs.register();
        self.queue_depth.register();
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

    pub fn set_queue_depth(&self, value: usize) {
        self.queue_depth.set_lossy(value);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_queue_depth_value(&self, metrics_as_string: &str) -> usize {
        self.queue_depth
            .parse_numeric_metric::<usize>(metrics_as_string)
            .expect("queue_depth metrics should be available")
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

    pub fn get_processed_metric(&self) -> &'static MetricCounter {
        self.processed_msgs
    }

    pub fn get_queue_depth_metric(&self) -> &'static MetricGauge {
        self.queue_depth
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

    pub fn get_valid_received_metric(&self) -> &'static MetricCounter {
        self.valid_received_msgs
    }

    pub fn get_processed_metric(&self) -> &'static MetricCounter {
        self.processed_msgs
    }

    pub fn get_number_of_connections_metric(&self) -> &'static MetricGauge {
        self.number_of_connections
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
