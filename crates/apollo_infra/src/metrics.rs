use apollo_metrics::define_metrics;
use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricGauge,
    MetricHistogram,
};

use crate::requests::LABEL_NAME_REQUEST_VARIANT;

define_metrics!(
    Infra => {
        // Local server counters
        MetricCounter { BATCHER_LOCAL_MSGS_RECEIVED, "batcher_local_msgs_received", "Counter of messages received by batcher local server", init = 0 },
        MetricCounter { BATCHER_LOCAL_MSGS_PROCESSED, "batcher_local_msgs_processed", "Counter of messages processed by batcher local server", init = 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_RECEIVED, "class_manager_local_msgs_received", "Counter of messages received by class manager local server", init = 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_PROCESSED, "class_manager_local_msgs_processed", "Counter of messages processed by class manager local server", init = 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_RECEIVED, "gateway_local_msgs_received", "Counter of messages received by gateway local server", init = 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_PROCESSED, "gateway_local_msgs_processed", "Counter of messages processed by gateway local server", init = 0 },
        MetricCounter { L1_ENDPOINT_MONITOR_LOCAL_MSGS_RECEIVED, "l1_endpoint_monitor_local_msgs_received", "Counter of messages received by L1 endpoint monitor local server", init = 0 },
        MetricCounter { L1_ENDPOINT_MONITOR_LOCAL_MSGS_PROCESSED, "l1_endpoint_monitor_local_msgs_processed", "Counter of messages processed by L1 endpoint monitor local server", init = 0 },
        MetricCounter { L1_PROVIDER_LOCAL_MSGS_RECEIVED, "l1_provider_local_msgs_received", "Counter of messages received by L1 provider local server", init = 0 },
        MetricCounter { L1_PROVIDER_LOCAL_MSGS_PROCESSED, "l1_provider_local_msgs_processed", "Counter of messages processed by L1 provider local server", init = 0 },
        MetricCounter { L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED, "l1_gas_price_provider_local_msgs_received", "Counter of messages received by L1 gas price provider local server", init = 0 },
        MetricCounter { L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED, "l1_gas_price_provider_local_msgs_processed", "Counter of messages processed by L1 gas price provider local server", init = 0 },
        MetricCounter { MEMPOOL_LOCAL_MSGS_RECEIVED, "mempool_local_msgs_received", "Counter of messages received by mempool local server", init = 0 },
        MetricCounter { MEMPOOL_LOCAL_MSGS_PROCESSED, "mempool_local_msgs_processed", "Counter of messages processed by mempool local server", init = 0 },
        MetricCounter { MEMPOOL_P2P_LOCAL_MSGS_RECEIVED, "mempool_p2p_propagator_local_msgs_received", "Counter of messages received by mempool p2p local server", init = 0 },
        MetricCounter { MEMPOOL_P2P_LOCAL_MSGS_PROCESSED, "mempool_p2p_propagator_local_msgs_processed", "Counter of messages processed by mempool p2p local server", init = 0 },
        MetricCounter { SIERRA_COMPILER_LOCAL_MSGS_RECEIVED, "sierra_compiler_local_msgs_received", "Counter of messages received by sierra compiler local server", init = 0 },
        MetricCounter { SIERRA_COMPILER_LOCAL_MSGS_PROCESSED, "sierra_compiler_local_msgs_processed", "Counter of messages processed by sierra compiler local server", init = 0 },
        MetricCounter { STATE_SYNC_LOCAL_MSGS_RECEIVED, "state_sync_local_msgs_received", "Counter of messages received by state sync local server", init = 0 },
        MetricCounter { STATE_SYNC_LOCAL_MSGS_PROCESSED, "state_sync_local_msgs_processed", "Counter of messages processed by state sync local server", init = 0 },
        // Remote server counters
        MetricCounter { BATCHER_REMOTE_MSGS_RECEIVED, "batcher_remote_msgs_received", "Counter of messages received by batcher remote server", init = 0 },
        MetricCounter { BATCHER_REMOTE_VALID_MSGS_RECEIVED, "batcher_remote_valid_msgs_received", "Counter of valid messages received by batcher remote server", init = 0 },
        MetricCounter { BATCHER_REMOTE_MSGS_PROCESSED, "batcher_remote_msgs_processed", "Counter of messages processed by batcher remote server", init = 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_MSGS_RECEIVED, "class_manager_remote_msgs_received", "Counter of messages received by class manager remote server", init = 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, "class_manager_remote_valid_msgs_received", "Counter of valid messages received by class manager remote server", init = 0 },
        MetricCounter { CLASS_MANAGER_REMOTE_MSGS_PROCESSED, "class_manager_remote_msgs_processed", "Counter of messages processed by class manager remote server", init = 0 },
        MetricCounter { GATEWAY_REMOTE_MSGS_RECEIVED, "gateway_remote_msgs_received", "Counter of messages received by gateway remote server", init = 0 },
        MetricCounter { GATEWAY_REMOTE_VALID_MSGS_RECEIVED, "gateway_remote_valid_msgs_received", "Counter of valid messages received by gateway remote server", init = 0 },
        MetricCounter { GATEWAY_REMOTE_MSGS_PROCESSED, "gateway_remote_msgs_processed", "Counter of messages processed by gateway remote server", init = 0 },
        MetricCounter { L1_ENDPOINT_MONITOR_REMOTE_MSGS_RECEIVED, "l1_endpoint_monitor_remote_msgs_received", "Counter of messages received by L1 endpoint monitor remote server", init = 0 },
        MetricCounter { L1_ENDPOINT_MONITOR_REMOTE_VALID_MSGS_RECEIVED, "l1_endpoint_monitor_remote_valid_msgs_received", "Counter of valid messages received by L1 endpoint monitor remote server", init = 0 },
        MetricCounter { L1_ENDPOINT_MONITOR_REMOTE_MSGS_PROCESSED, "l1_endpoint_monitor_remote_msgs_processed", "Counter of messages processed by L1 endpoint monitor remote server", init = 0 },
        MetricCounter { L1_PROVIDER_REMOTE_MSGS_RECEIVED, "l1_provider_remote_msgs_received", "Counter of messages received by L1 provider remote server", init = 0 },
        MetricCounter { L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, "l1_provider_remote_valid_msgs_received", "Counter of valid messages received by L1 provider remote server", init = 0 },
        MetricCounter { L1_PROVIDER_REMOTE_MSGS_PROCESSED, "l1_provider_remote_msgs_processed", "Counter of messages processed by L1 provider remote server", init = 0 },
        MetricCounter { L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED, "l1_gas_price_provider_remote_msgs_received", "Counter of messages received by L1 gas price provider remote server", init = 0 },
        MetricCounter { L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, "l1_gas_price_provider_remote_valid_msgs_received", "Counter of valid messages received by L1 gas price provider remote server", init = 0 },
        MetricCounter { L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED, "l1_gas_price_provider_remote_msgs_processed", "Counter of messages processed by L1 gas price provider remote server", init = 0 },
        MetricCounter { MEMPOOL_REMOTE_MSGS_RECEIVED, "mempool_remote_msgs_received", "Counter of messages received by mempool remote server", init = 0 },
        MetricCounter { MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, "mempool_remote_valid_msgs_received", "Counter of valid messages received by mempool remote server", init = 0 },
        MetricCounter { MEMPOOL_REMOTE_MSGS_PROCESSED, "mempool_remote_msgs_processed", "Counter of messages processed by mempool remote server", init = 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, "mempool_p2p_propagator_remote_msgs_received", "Counter of messages received by mempool p2p remote server", init = 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, "mempool_p2p_propagator_remote_valid_msgs_received", "Counter of valid messages received by mempool p2p remote server", init = 0 },
        MetricCounter { MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, "mempool_p2p_propagator_remote_msgs_processed", "Counter of messages processed by mempool p2p remote server", init = 0 },
        MetricCounter { SIERRA_COMPILER_REMOTE_MSGS_RECEIVED, "sierra_compiler_remote_msgs_received", "Counter of messages received by sierra compiler remote server", init = 0 },
        MetricCounter { SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED, "sierra_compiler_remote_valid_msgs_received", "Counter of valid messages received by sierra compiler remote server", init = 0 },
        MetricCounter { SIERRA_COMPILER_REMOTE_MSGS_PROCESSED, "sierra_compiler_remote_msgs_processed", "Counter of messages processed by sierra compiler remote server", init = 0 },
        MetricCounter { STATE_SYNC_REMOTE_MSGS_RECEIVED, "state_sync_remote_msgs_received", "Counter of messages received by state sync remote server", init = 0 },
        MetricCounter { STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, "state_sync_remote_valid_msgs_received", "Counter of valid messages received by state sync remote server", init = 0 },
        MetricCounter { STATE_SYNC_REMOTE_MSGS_PROCESSED, "state_sync_remote_msgs_processed", "Counter of messages processed by state sync remote server", init = 0 },
        // Remote server gauges
        MetricGauge { BATCHER_REMOTE_NUMBER_OF_CONNECTIONS, "batcher_remote_number_of_connections", "Number of connections to batcher remote server" },
        MetricGauge { CLASS_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS, "class_manager_remote_number_of_connections", "Number of connections to class manager remote server" },
        MetricGauge { GATEWAY_REMOTE_NUMBER_OF_CONNECTIONS, "gateway_remote_number_of_connections", "Number of connections to gateway remote server" },
        MetricGauge { L1_ENDPOINT_MONITOR_REMOTE_NUMBER_OF_CONNECTIONS, "l1_endpoint_monitor_remote_number_of_connections", "Number of connections to L1 endpoint monitor remote server" },
        MetricGauge { L1_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS, "l1_provider_remote_number_of_connections", "Number of connections to L1 provider remote server" },
        MetricGauge { L1_GAS_PRICE_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS, "l1_gas_price_provider_remote_number_of_connections", "Number of connections to L1 gas price provider remote server" },
        MetricGauge { MEMPOOL_REMOTE_NUMBER_OF_CONNECTIONS, "mempool_remote_number_of_connections", "Number of connections to mempool remote server" },
        MetricGauge { MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS, "mempool_p2p_propagator_remote_number_of_connections", "Number of connections to mempool p2p remote server" },
        MetricGauge { SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS, "sierra_compiler_remote_number_of_connections", "Number of connections to sierra compiler remote server" },
        MetricGauge { STATE_SYNC_REMOTE_NUMBER_OF_CONNECTIONS, "state_sync_remote_number_of_connections", "Number of connections to state sync remote server" },
        // Local server priority queue depths
        MetricGauge { BATCHER_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "batcher_local_high_priority_queue_depth", "The depth of the batcher's high priority request queue" },
        MetricGauge { BATCHER_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "batcher_local_normal_priority_queue_depth", "The depth of the batcher's normal priority request queue" },
        MetricGauge { CLASS_MANAGER_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "class_manager_local_high_priority_queue_depth", "The depth of the class manager's high priority request queue" },
        MetricGauge { CLASS_MANAGER_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "class_manager_local_normal_priority_queue_depth", "The depth of the class manager's normal priority request queue" },
        MetricGauge { GATEWAY_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "gateway_local_high_priority_queue_depth", "The depth of the gateway's high priority request queue" },
        MetricGauge { GATEWAY_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "gateway_local_normal_priority_queue_depth", "The depth of the gateway's normal priority request queue" },
        MetricGauge { L1_ENDPOINT_MONITOR_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "l1_endpoint_monitor_local_high_priority_queue_depth", "The depth of the L1 endpoint monitor's high priority request queue" },
        MetricGauge { L1_ENDPOINT_MONITOR_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "l1_endpoint_monitor_local_normal_priority_queue_depth", "The depth of the L1 endpoint monitor's normal priority request queue" },
        MetricGauge { L1_PROVIDER_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "l1_provider_local_high_priority_queue_depth", "The depth of the L1 provider's high priority request queue" },
        MetricGauge { L1_PROVIDER_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "l1_provider_local_normal_priority_queue_depth", "The depth of the L1 provider's normal priority request queue" },
        MetricGauge { L1_GAS_PRICE_PROVIDER_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "l1_gas_price_provider_local_high_priority_queue_depth", "The depth of the L1 gas price provider's high priority request queue" },
        MetricGauge { L1_GAS_PRICE_PROVIDER_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "l1_gas_price_provider_local_normal_priority_queue_depth", "The depth of the L1 gas price provider's normal priority request queue" },
        MetricGauge { MEMPOOL_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "mempool_local_high_priority_queue_depth", "The depth of the mempool's high priority request queue" },
        MetricGauge { MEMPOOL_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "mempool_local_normal_priority_queue_depth", "The depth of the mempool's normal priority request queue" },
        MetricGauge { MEMPOOL_P2P_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "mempool_p2p_propagator_local_high_priority_queue_depth", "The depth of the mempool p2p's high priority request queue" },
        MetricGauge { MEMPOOL_P2P_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "mempool_p2p_propagator_local_normal_priority_queue_depth", "The depth of the mempool p2p's normal priority request queue" },
        MetricGauge { SIERRA_COMPILER_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "sierra_compiler_local_high_priority_queue_depth", "The depth of the sierra compiler's high priority request queue" },
        MetricGauge { SIERRA_COMPILER_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "sierra_compiler_local_normal_priority_queue_depth", "The depth of the sierra compiler's normal priority request queue" },
        MetricGauge { STATE_SYNC_LOCAL_HIGH_PRIORITY_QUEUE_DEPTH, "state_sync_local_high_priority_queue_depth", "The depth of the state sync's high priority request queue" },
        MetricGauge { STATE_SYNC_LOCAL_NORMAL_PRIORITY_QUEUE_DEPTH, "state_sync_local_normal_priority_queue_depth", "The depth of the state sync's normal priority request queue" },
        // Remote client metrics
        MetricHistogram { BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS, "batcher_remote_client_send_attempts", "Required number of remote connection attempts made by a batcher remote client"},
        MetricHistogram { CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS, "class_manager_remote_client_send_attempts", "Required number of remote connection attempts made by a class manager remote client"},
        MetricHistogram { GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS, "gateway_remote_client_send_attempts", "Required number of remote connection attempts made by a gateway remote client"},
        MetricHistogram { L1_ENDPOINT_MONITOR_SEND_ATTEMPTS, "l1_endpoint_monitor_remote_client_send_attempts", "Required number of remote connection attempts made by a L1 endpoint monitor remote client"},
        MetricHistogram { L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS, "l1_provider_remote_client_send_attempts", "Required number of remote connection attempts made by a L1 provider remote client"},
        MetricHistogram { L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS, "l1_gas_price_provider_remote_client_send_attempts", "Required number of remote connection attempts made by a L1 gas price provider remote client"},
        MetricHistogram { MEMPOOL_REMOTE_CLIENT_SEND_ATTEMPTS, "mempool_remote_client_send_attempts", "Required number of remote connection attempts made by a mempool remote client"},
        MetricHistogram { MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS, "mempool_p2p_propagator_remote_client_send_attempts", "Required number of remote connection attempts made by a mempool p2p remote client"},
        MetricHistogram { SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS, "sierra_compiler_remote_client_send_attempts", "Required number of remote connection attempts made by a sierra compiler remote client"},
        MetricHistogram { STATE_SYNC_REMOTE_CLIENT_SEND_ATTEMPTS, "state_sync_remote_client_send_attempts", "Required number of remote connection attempts made by a state sync remote client"},
    },
);
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

    pub fn _get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
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

    pub fn get_attempts_metric(&self) -> &'static MetricHistogram {
        self.attempts
    }

    pub fn get_response_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.response_times
    }

    pub fn get_communication_failure_time_metric(&self) -> &'static LabeledMetricHistogram {
        self.communication_failure_times
    }

    pub fn _get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
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

    #[cfg(any(feature = "testing", test))]
    pub fn get_total_queue_depth_value(&self, metrics_as_string: &str) -> usize {
        self.get_high_priority_queue_depth_value(metrics_as_string)
            + self.get_normal_priority_queue_depth_value(metrics_as_string)
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

    pub fn get_high_priority_queue_depth_metric(&self) -> &'static MetricGauge {
        self.high_priority_queue_depth
    }

    pub fn get_normal_priority_queue_depth_metric(&self) -> &'static MetricGauge {
        self.normal_priority_queue_depth
    }

    pub fn _get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
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

    pub fn get_valid_received_metric(&self) -> &'static MetricCounter {
        self.valid_received_msgs
    }

    pub fn get_processed_metric(&self) -> &'static MetricCounter {
        self.processed_msgs
    }

    pub fn get_number_of_connections_metric(&self) -> &'static MetricGauge {
        self.number_of_connections
    }

    pub fn _get_all_labeled_metrics(&self) -> Vec<&'static LabeledMetricHistogram> {
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
