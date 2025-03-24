use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Infra => {
        // Local server counters
        MetricCounter { BATCHER_LOCAL_MSGS_RECEIVED, "batcher_local_msgs_received", "Counter of messages received by batcher local server", init = 0 },
        MetricCounter { BATCHER_LOCAL_MSGS_PROCESSED, "batcher_local_msgs_processed", "Counter of messages processed by batcher local server", init = 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_RECEIVED, "class_manager_local_msgs_received", "Counter of messages received by class manager local server", init = 0 },
        MetricCounter { CLASS_MANAGER_LOCAL_MSGS_PROCESSED, "class_manager_local_msgs_processed", "Counter of messages processed by class manager local server", init = 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_RECEIVED, "gateway_local_msgs_received", "Counter of messages received by gateway local server", init = 0 },
        MetricCounter { GATEWAY_LOCAL_MSGS_PROCESSED, "gateway_local_msgs_processed", "Counter of messages processed by gateway local server", init = 0 },
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
        MetricCounter { STATE_SYNC_REMOTE_MSGS_RECEIVED, "state_sync_remote_msgs_received", "Counter of messages received by state sync remote server", init = 0 },
        MetricCounter { STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, "state_sync_remote_valid_msgs_received", "Counter of valid messages received by state sync remote server", init = 0 },
        MetricCounter { STATE_SYNC_REMOTE_MSGS_PROCESSED, "state_sync_remote_msgs_processed", "Counter of messages processed by state sync remote server", init = 0 },
        // Local server queue depths
        MetricGauge { BATCHER_LOCAL_QUEUE_DEPTH, "batcher_local_queue_depth", "The depth of the batcher's local message queue" },
        MetricGauge { CLASS_MANAGER_LOCAL_QUEUE_DEPTH, "class_manager_local_queue_depth", "The depth of the class manager's local message queue" },
        MetricGauge { GATEWAY_LOCAL_QUEUE_DEPTH, "gateway_local_queue_depth", "The depth of the gateway's local message queue" },
        MetricGauge { L1_PROVIDER_LOCAL_QUEUE_DEPTH, "l1_provider_local_queue_depth", "The depth of the L1 provider's local message queue" },
        MetricGauge { L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH, "l1_gas_price_provider_local_queue_depth", "The depth of the L1 gas price provider's local message queue" },
        MetricGauge { MEMPOOL_LOCAL_QUEUE_DEPTH, "mempool_local_queue_depth", "The depth of the mempool's local message queue" },
        MetricGauge { MEMPOOL_P2P_LOCAL_QUEUE_DEPTH, "mempool_p2p_propagator_local_queue_depth", "The depth of the mempool p2p's local message queue" },
        MetricGauge { SIERRA_COMPILER_LOCAL_QUEUE_DEPTH, "sierra_compiler_local_queue_depth", "The depth of the sierra compiler's local message queue" },
        MetricGauge { STATE_SYNC_LOCAL_QUEUE_DEPTH, "state_sync_local_queue_depth", "The depth of the state sync's local message queue" },
    },
);

/// A struct to contain all metrics for a local server.
pub struct LocalServerMetrics {
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
}

impl LocalServerMetrics {
    pub const fn new(
        received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        queue_depth: &'static MetricGauge,
    ) -> Self {
        Self { received_msgs, processed_msgs, queue_depth }
    }

    pub fn register(&self) {
        self.received_msgs.register();
        self.processed_msgs.register();
        self.queue_depth.register();
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
}

/// A struct to contain all metrics for a remote server.
pub struct RemoteServerMetrics {
    total_received_msgs: &'static MetricCounter,
    valid_received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
}

impl RemoteServerMetrics {
    pub const fn new(
        total_received_msgs: &'static MetricCounter,
        valid_received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
    ) -> Self {
        Self { total_received_msgs, valid_received_msgs, processed_msgs }
    }

    pub fn register(&self) {
        self.total_received_msgs.register();
        self.valid_received_msgs.register();
        self.processed_msgs.register();
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
}
