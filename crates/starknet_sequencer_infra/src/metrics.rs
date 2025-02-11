use std::collections::HashMap;

use starknet_infra_utils::type_name::camel_to_snake_case;
use starknet_sequencer_metrics::metric_definitions::{METRIC_COUNTERS_MAP, METRIC_GAUGES_MAP};
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge, MetricScope};
use tracing::warn;

/// A struct to contain all metrics for the a server/component.
pub struct InfraMetrics {
    mock: bool,
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
    queue_depth: &'static MetricGauge,
}

const MOCK_METRIC_COUNTER: MetricCounter =
    MetricCounter::new(MetricScope::Infra, "mock_test_counter", "Mock test counter", 0);

const MOCK_METRIC_GAUGE: MetricGauge =
    MetricGauge::new(MetricScope::Infra, "mock_test_gauge", "Mock test gauge");

fn get_metrics_object<T>(
    metrics_name: &str,
    metrics_type: &str,
    metrics_object_map: &HashMap<&str, &'static T>,
) -> Option<&'static T> {
    let metrics = metrics_object_map.get(metrics_name);
    if metrics.is_none() {
        warn!("{} {} not found", metrics_type, metrics_name);
    }

    metrics.map(|v| &**v)
}

fn get_metrics_counter(metrics_name: &str) -> Option<&'static MetricCounter> {
    get_metrics_object(metrics_name, "Counter", &METRIC_COUNTERS_MAP)
}

fn get_metrics_gauge(metrics_name: &str) -> Option<&'static MetricGauge> {
    get_metrics_object(metrics_name, "Gauge", &METRIC_GAUGES_MAP)
}

fn get_received_msgs_counter(name: &str) -> Option<&'static MetricCounter> {
    get_metrics_counter(&format!("{}_msgs_received", name))
}

fn get_processed_msgs_counter(name: &str) -> Option<&'static MetricCounter> {
    get_metrics_counter(&format!("{}_msgs_processed", name))
}

fn get_queue_depth_gauge(name: &str) -> Option<&'static MetricGauge> {
    get_metrics_gauge(&format!("{}_queue_depth", name))
}

impl InfraMetrics {
    pub fn new(component_name: &str) -> Self {
        let snake_case_component_name = camel_to_snake_case(component_name);

        let received_msgs = get_received_msgs_counter(&snake_case_component_name);
        let processed_msgs = get_processed_msgs_counter(&snake_case_component_name);
        let queue_depth = get_queue_depth_gauge(&snake_case_component_name);

        let infra_metrics =
            match received_msgs.is_none() || processed_msgs.is_none() || queue_depth.is_none() {
                true => InfraMetrics {
                    mock: true,
                    received_msgs: &MOCK_METRIC_COUNTER,
                    processed_msgs: &MOCK_METRIC_COUNTER,
                    queue_depth: &MOCK_METRIC_GAUGE,
                },
                false => InfraMetrics {
                    mock: false,
                    received_msgs: received_msgs.unwrap(),
                    processed_msgs: processed_msgs.unwrap(),
                    queue_depth: queue_depth.unwrap(),
                },
            };

        infra_metrics.register_infra_metrics();

        infra_metrics
    }

    fn register_infra_metrics(&self) {
        if !self.mock {
            self.received_msgs.register();
            self.processed_msgs.register();
        }
    }

    pub fn increment_received(&self) {
        if !self.mock {
            self.received_msgs.increment(1);
        }
    }

    pub fn increment_processed(&self) {
        if !self.mock {
            self.processed_msgs.increment(1);
        }
    }

    #[allow(clippy::as_conversions)]
    pub fn set_queue_depth(&self, value: usize) {
        if !self.mock {
            self.queue_depth.set(value as f64);
        }
    }
}
