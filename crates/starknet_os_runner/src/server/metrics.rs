//! Metrics for the proving service.
//!
//! This module defines proving-specific metrics using `apollo_metrics::define_metrics!`.

use apollo_metrics::define_metrics;

define_metrics!(
    ProvingServer => {
        // Request counters.
        MetricCounter {
            PROVING_REQUESTS_TOTAL,
            "proving_requests_total",
            "Total number of proving requests received",
            init = 0
        },
        MetricCounter {
            PROVING_REQUESTS_SUCCESS,
            "proving_requests_success",
            "Number of successful proving requests",
            init = 0
        },
        MetricCounter {
            PROVING_REQUESTS_FAILURE,
            "proving_requests_failure",
            "Number of failed proving requests",
            init = 0
        },
        // Latency histograms.
        MetricHistogram {
            PROVING_OS_EXECUTION_LATENCY,
            "proving_os_execution_latency_seconds",
            "Histogram of OS execution latency in seconds"
        },
        MetricHistogram {
            PROVING_PROVER_LATENCY,
            "proving_prover_latency_seconds",
            "Histogram of prover latency in seconds"
        },
        MetricHistogram {
            PROVING_TOTAL_LATENCY,
            "proving_total_latency_seconds",
            "Histogram of end-to-end proving latency in seconds"
        },
    },
);

/// Registers all proving metrics.
pub fn register_metrics() {
    PROVING_REQUESTS_TOTAL.register();
    PROVING_REQUESTS_SUCCESS.register();
    PROVING_REQUESTS_FAILURE.register();
    PROVING_OS_EXECUTION_LATENCY.register();
    PROVING_PROVER_LATENCY.register();
    PROVING_TOTAL_LATENCY.register();
}
