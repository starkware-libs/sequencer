use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;

use crate::tokio_metrics::{
    setup_tokio_metrics,
    TOKIO_GLOBAL_QUEUE_DEPTH,
    TOKIO_MAX_BUSY_DURATION_MICROS,
    TOKIO_MAX_PARK_COUNT,
    TOKIO_MIN_BUSY_DURATION_MICROS,
    TOKIO_MIN_PARK_COUNT,
    TOKIO_TOTAL_BUSY_DURATION_MICROS,
    TOKIO_TOTAL_PARK_COUNT,
    TOKIO_WORKERS_COUNT,
};

#[tokio::test]
async fn tokio_metrics_present() {
    // Create a local recorder instead of installing a global one
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    // Setup tokio metrics collection with the local recorder
    setup_tokio_metrics();

    // Allow the exporter to export tokio metrics
    tokio::task::yield_now().await;

    // Get the metrics directly from the local recorder
    let prometheus_output = recorder.handle().render();

    TOKIO_TOTAL_BUSY_DURATION_MICROS.assert_exists(&prometheus_output);
    TOKIO_MIN_BUSY_DURATION_MICROS.assert_exists(&prometheus_output);
    TOKIO_MAX_BUSY_DURATION_MICROS.assert_exists(&prometheus_output);
    TOKIO_TOTAL_PARK_COUNT.assert_exists(&prometheus_output);
    TOKIO_MIN_PARK_COUNT.assert_exists(&prometheus_output);
    TOKIO_MAX_PARK_COUNT.assert_exists(&prometheus_output);
    TOKIO_WORKERS_COUNT.assert_exists(&prometheus_output);
    TOKIO_GLOBAL_QUEUE_DEPTH.assert_exists(&prometheus_output);
}
