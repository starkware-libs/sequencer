use std::time::Duration;

use tokio_metrics::RuntimeMetricsReporterBuilder;

const TOKIO_REPORTING_INTERVAL_SECONDS: u64 = 10;

/// Start the tokio runtime metrics reporter to automatically collect and export tokio runtime
/// metrics
pub(crate) fn setup_tokio_metrics() {
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(TOKIO_REPORTING_INTERVAL_SECONDS))
            .describe_and_run(),
    );
}
