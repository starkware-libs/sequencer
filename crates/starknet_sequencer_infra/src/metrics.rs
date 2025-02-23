use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

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

    pub fn get_received_value(&self, metrics_as_string: &str) -> Option<u64> {
        self.received_msgs.parse_numeric_metric::<u64>(metrics_as_string)
    }

    pub fn increment_processed(&self) {
        self.processed_msgs.increment(1);
    }

    pub fn get_processed_value(&self, metrics_as_string: &str) -> Option<u64> {
        self.processed_msgs.parse_numeric_metric::<u64>(metrics_as_string)
    }

    #[allow(clippy::as_conversions)]
    pub fn set_queue_depth(&self, value: usize) {
        // TODO(Itay,Lev): Enhance the gauge interface to support taking usize args.
        self.queue_depth.set(value as f64);
    }

    pub fn get_queue_depth_value(&self, metrics_as_string: &str) -> Option<usize> {
        self.queue_depth.parse_numeric_metric::<usize>(metrics_as_string)
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

    pub fn get_total_received_value(&self, metrics_as_string: &str) -> Option<u64> {
        self.total_received_msgs.parse_numeric_metric::<u64>(metrics_as_string)
    }

    pub fn increment_valid_received(&self) {
        self.valid_received_msgs.increment(1);
    }

    pub fn get_valid_received_value(&self, metrics_as_string: &str) -> Option<u64> {
        self.valid_received_msgs.parse_numeric_metric::<u64>(metrics_as_string)
    }

    pub fn increment_processed(&self) {
        self.processed_msgs.increment(1);
    }

    pub fn get_processed_value(&self, metrics_as_string: &str) -> Option<u64> {
        self.processed_msgs.parse_numeric_metric::<u64>(metrics_as_string)
    }
}
