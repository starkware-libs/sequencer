use std::sync::Arc;

use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge, MetricScope};

use crate::metrics::{create_shared_infra_metrics, InfraMetrics};

const MOCK_MSGS_RECEIVED: MetricCounter =
    MetricCounter::new(MetricScope::Infra, "mock_test_counter", "Mock test counter", 0);

const MOCK_MSGS_PROCESSED: MetricCounter =
    MetricCounter::new(MetricScope::Infra, "mock_test_counter", "Mock test counter", 0);

const MOCK_QUEUE_DEPTH: MetricGauge =
    MetricGauge::new(MetricScope::Infra, "mock_test_gauge", "Mock test gauge");

pub fn create_shared_mock_metrics() -> Arc<InfraMetrics> {
    create_shared_infra_metrics(&MOCK_MSGS_RECEIVED, &MOCK_MSGS_PROCESSED, &MOCK_QUEUE_DEPTH)
}
