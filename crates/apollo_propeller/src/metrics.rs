//! Metrics for the Propeller protocol.
//!
//! This module provides metrics for monitoring the Propeller protocol's performance.

use apollo_metrics::metrics::MetricCounter;

/// Propeller protocol metrics
pub struct PropellerMetrics {
    /// Total number of shards received from peers
    pub shards_received: MetricCounter,
}

impl PropellerMetrics {
    /// Register all metrics with the metrics system
    pub fn register(&self) {
        self.shards_received.register();
    }
}
