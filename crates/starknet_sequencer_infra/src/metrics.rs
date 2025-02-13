use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

/// A struct to contain all metrics for the a server/component.
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
        // TODO(Itay,Lev): Enhance the gauge interface to support taking usize args.
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
        // TODO(Itay,Lev): Enhance the gauge Fix gauge set in the starknet_sequencer_metrics.
        self.queue_depth.set(value as f64);
    }
}

pub fn bundle_local_server_metrics(
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
) -> LocalServerMetrics {
    LocalServerMetrics::new(received_msgs, processed_msgs, queue_depth)
}
