use std::sync::Arc;

use crate::metrics::InfraMetricsTrait;

pub struct MockMetrics {}

impl MockMetrics {
    pub fn new() -> Self {
        Self {}
    }
}

pub fn create_shared_mock_metrics() -> Arc<MockMetrics> {
    Arc::new(MockMetrics::new())
}

impl InfraMetricsTrait for MockMetrics {
    fn register_infra_metrics(&self) {}

    fn increment_received(&self) {}

    fn increment_processed(&self) {}

    fn set_queue_depth(&self, _value: usize) {}
}
