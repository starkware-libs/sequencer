use std::time::Duration;

use tokio_metrics::RuntimeMetricsReporterBuilder;

/// Start the tokio runtime metrics reporter to automatically collect and export tokio runtime
/// metrics
pub(crate) fn setup_tokio_metrics() {
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(10))
            .describe_and_run(),
    );
}
