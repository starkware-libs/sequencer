//! Metrics for the Propeller protocol.
//!
//! This module provides metrics for monitoring the Propeller protocol's performance.

use apollo_metrics::metrics::MetricCounter;

/// Propeller protocol metrics
pub struct PropellerMetrics {
    /// Total number of units received from peers
    pub units_received: MetricCounter,
}

impl PropellerMetrics {
    /// Register all metrics with the metrics system
    pub fn register(&self) {
        self.units_received.register();
    }
}
