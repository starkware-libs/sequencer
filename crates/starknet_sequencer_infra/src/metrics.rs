use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

/// A struct to contain all metrics for a local server.
pub struct LocalServerMetrics {
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
}

impl LocalServerMetrics {
    pub fn new(
        received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        queue_depth: &'static MetricGauge,
    ) -> Self {
        let infra_metrics = Self { received_msgs, processed_msgs, queue_depth };

        infra_metrics.register();

        infra_metrics
    }

    fn register(&self) {
        self.received_msgs.register();
        self.processed_msgs.register();
        let _ = self.queue_depth.register();
    }

    pub fn increment_received(&self) {
        self.received_msgs.increment(1);
    }

    pub fn increment_processed(&self) {
        self.processed_msgs.increment(1);
    }

    #[allow(clippy::as_conversions)]
    pub fn set_queue_depth(&self, value: usize) {
        // TODO(Itay,Lev): Enhance the gauge interface to support taking usize args.
        self.queue_depth.set(value as f64);
    }
}
