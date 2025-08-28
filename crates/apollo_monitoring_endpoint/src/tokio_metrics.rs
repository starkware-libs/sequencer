use std::time::Duration;

use apollo_metrics::define_metrics;
use tokio_metrics::RuntimeMetricsReporterBuilder;

define_metrics!(
    Infra => {
        // Counters (duration metrics in microseconds)
        MetricCounter { TOKIO_TOTAL_BUSY_DURATION, "tokio_total_busy_duration", "The amount of time worker threads were busy (in microseconds)", init = 0 },
        MetricCounter { TOKIO_MIN_BUSY_DURATION, "tokio_min_busy_duration", "The minimum amount of time a worker thread was busy (in microseconds)", init = 0 },
        MetricCounter { TOKIO_MAX_BUSY_DURATION, "tokio_max_busy_duration", "The maximum amount of time a worker thread was busy (in microseconds)", init = 0 },
        // Gauges (count metrics)
        MetricGauge { TOKIO_TOTAL_PARK_COUNT, "tokio_total_park_count", "The number of times worker threads parked" },
        MetricGauge { TOKIO_MIN_PARK_COUNT, "tokio_min_park_count", "The minimum number of times any worker thread parked" },
        MetricGauge { TOKIO_MAX_PARK_COUNT, "tokio_max_park_count", "The maximum number of times any worker thread parked" },
        MetricGauge { TOKIO_GLOBAL_QUEUE_DEPTH, "tokio_global_queue_depth", "The number of tasks currently scheduled in the runtime's global queue" },
        MetricGauge { TOKIO_WORKERS_COUNT, "tokio_workers_count", "The number of worker threads used by the runtime" },
    },
);

/// Start the tokio runtime metrics reporter to automatically collect and export tokio runtime
/// metrics
pub(crate) fn setup_tokio_metrics() {
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(1))
            .describe_and_run(),
    );
}
