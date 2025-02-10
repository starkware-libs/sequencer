use starknet_infra_utils::type_name::camel_to_snake_case;
use starknet_sequencer_metrics::metric_definitions::METRIC_COUNTERS_MAP;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricScope};
use tracing::warn;

/// A struct to contain all metrics for the a server/component.
pub struct InfraMetrics {
    mock: bool,
    received_msgs: &'static MetricCounter,
    processed_msgs: &'static MetricCounter,
}

const MOCK_METRIC_COUNTER: MetricCounter =
    MetricCounter::new(MetricScope::Infra, "fake_test_counter", "Fake test counter", 0);

fn get_metrics_counter(name: &str) -> Option<&'static MetricCounter> {
    let counter = METRIC_COUNTERS_MAP.get(name);
    if counter.is_none() {
        warn!("Counter {} not found", name);
    }

    counter.map(|v| &**v)
}

fn get_received_msgs_counter(name: &str) -> Option<&'static MetricCounter> {
    get_metrics_counter(&format!("{}_msgs_received", name))
}

fn get_processed_msgs_counter(name: &str) -> Option<&'static MetricCounter> {
    get_metrics_counter(&format!("{}_msgs_processed", name))
}

impl InfraMetrics {
    pub fn new(component_name: &str) -> Self {
        let snake_case_component_name = camel_to_snake_case(component_name);
        let received_msgs = get_received_msgs_counter(&snake_case_component_name);
        let processed_msgs = get_processed_msgs_counter(&snake_case_component_name);

        let infra_metrics = match received_msgs.is_none() || processed_msgs.is_none() {
            true => InfraMetrics {
                mock: true,
                received_msgs: &MOCK_METRIC_COUNTER,
                processed_msgs: &MOCK_METRIC_COUNTER,
            },
            false => InfraMetrics {
                mock: false,
                received_msgs: received_msgs.unwrap(),
                processed_msgs: processed_msgs.unwrap(),
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
}
