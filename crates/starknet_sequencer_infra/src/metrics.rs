use std::sync::Arc;

use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

/// A struct to contain all metrics for the a server/component.
pub struct InfraMetrics {
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
}

impl InfraMetrics {
    pub fn new(
        received_msgs: &'static MetricCounter,
        processed_msgs: &'static MetricCounter,
        queue_depth: &'static MetricGauge,
    ) -> Self {
        let infra_metrics = Self { received_msgs, processed_msgs, queue_depth };

        infra_metrics.register_infra_metrics();

        infra_metrics
    }

    fn register_infra_metrics(&self) {
        self.received_msgs.register();
        self.processed_msgs.register();
        // TODO(Itay,Lev): Fix gauge register in the starknet_sequencer_metrics.
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
        self.queue_depth.set(value as f64);
    }
}

pub fn create_shared_infra_metrics(
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
) -> Arc<InfraMetrics> {
    Arc::new(InfraMetrics::new(received_msgs, processed_msgs, queue_depth))
}
