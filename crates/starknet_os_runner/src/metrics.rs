//! OpenTelemetry metrics for the proving service.
//!
//! When a metrics endpoint is configured (via `--metrics-endpoint` or
//! `OTEL_EXPORTER_OTLP_ENDPOINT`), metrics are pushed over OTLP.
//! Otherwise, all instruments are no-ops with zero overhead.

use opentelemetry::metrics::{Counter, Histogram, MeterProvider, UpDownCounter};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::SdkMeterProvider;

const METER_NAME: &str = "proving_service";

/// Holds all metric instruments for the proving service.
#[derive(Clone)]
pub struct ProvingMetrics {
    /// Total prove_transaction requests received.
    requests_received: Counter<u64>,
    /// Successful requests.
    requests_succeeded: Counter<u64>,
    /// Rejected requests (at capacity).
    requests_rejected: Counter<u64>,
    /// Failed requests (with error_type attribute).
    requests_failed: Counter<u64>,
    /// Currently in-flight proving requests.
    concurrent_requests: UpDownCounter<i64>,
    /// OS execution time in seconds.
    os_execution_duration: Histogram<f64>,
    /// Stwo prover time in seconds.
    proving_duration: Histogram<f64>,
    /// End-to-end request time in seconds.
    request_duration: Histogram<f64>,
    /// Cairo VM steps per execution.
    os_execution_steps: Histogram<u64>,
}

impl ProvingMetrics {
    /// Creates instruments from the given meter provider.
    fn new(meter_provider: &SdkMeterProvider) -> Self {
        let meter = meter_provider.meter(METER_NAME);

        let requests_received = meter
            .u64_counter("proving_service.requests.received")
            .with_description("Total prove_transaction requests received")
            .build();

        let requests_succeeded = meter
            .u64_counter("proving_service.requests.succeeded")
            .with_description("Successful requests")
            .build();

        let requests_rejected = meter
            .u64_counter("proving_service.requests.rejected")
            .with_description("Rejected requests (at capacity)")
            .build();

        let requests_failed = meter
            .u64_counter("proving_service.requests.failed")
            .with_description("Failed requests")
            .build();

        let concurrent_requests = meter
            .i64_up_down_counter("proving_service.concurrent_requests")
            .with_description("Currently in-flight proving requests")
            .build();

        let os_execution_duration = meter
            .f64_histogram("proving_service.os_execution.duration")
            .with_description("OS execution time")
            .with_unit("s")
            .build();

        let proving_duration = meter
            .f64_histogram("proving_service.proving.duration")
            .with_description("Stwo prover time")
            .with_unit("s")
            .build();

        let request_duration = meter
            .f64_histogram("proving_service.request.duration")
            .with_description("End-to-end request time")
            .with_unit("s")
            .build();

        let os_execution_steps = meter
            .u64_histogram("proving_service.os_execution.steps")
            .with_description("Cairo VM steps per execution")
            .build();

        Self {
            requests_received,
            requests_succeeded,
            requests_rejected,
            requests_failed,
            concurrent_requests,
            os_execution_duration,
            proving_duration,
            request_duration,
            os_execution_steps,
        }
    }

    /// Records that a new request was received.
    pub fn record_request_received(&self) {
        self.requests_received.add(1, &[]);
        self.concurrent_requests.add(1, &[]);
    }

    /// Records that a request completed successfully.
    pub fn record_request_succeeded(&self, duration_seconds: f64) {
        self.requests_succeeded.add(1, &[]);
        self.request_duration.record(duration_seconds, &[]);
        self.concurrent_requests.add(-1, &[]);
    }

    /// Records that a request was rejected because the service is at capacity.
    pub fn record_request_rejected(&self) {
        self.requests_rejected.add(1, &[]);
        self.concurrent_requests.add(-1, &[]);
    }

    /// Records that a request failed with the given error type.
    pub fn record_request_failed(&self, error_type: &str) {
        self.requests_failed.add(1, &[KeyValue::new("error_type", error_type.to_owned())]);
        self.concurrent_requests.add(-1, &[]);
    }

    /// Records OS execution duration in seconds.
    pub fn record_os_execution_duration(&self, duration_seconds: f64) {
        self.os_execution_duration.record(duration_seconds, &[]);
    }

    /// Records proving duration in seconds.
    pub fn record_proving_duration(&self, duration_seconds: f64) {
        self.proving_duration.record(duration_seconds, &[]);
    }

    /// Records the number of Cairo VM steps from an execution.
    pub fn record_os_execution_steps(&self, n_steps: u64) {
        self.os_execution_steps.record(n_steps, &[]);
    }
}

/// Initializes the metrics subsystem.
///
/// If `endpoint` is `Some`, configures an OTLP exporter pushing to that endpoint.
/// Otherwise, creates a no-op meter provider (zero overhead).
///
/// Returns the `ProvingMetrics` instruments and the `SdkMeterProvider` (needed for
/// shutdown).
pub fn init_metrics(endpoint: Option<&str>) -> (ProvingMetrics, SdkMeterProvider) {
    let meter_provider = match endpoint {
        Some(endpoint) => build_otlp_meter_provider(endpoint),
        None => SdkMeterProvider::builder().build(),
    };

    let metrics = ProvingMetrics::new(&meter_provider);
    (metrics, meter_provider)
}

/// Shuts down the meter provider, flushing any buffered metrics.
pub fn shutdown_metrics(meter_provider: SdkMeterProvider) {
    if let Err(err) = meter_provider.shutdown() {
        tracing::warn!(%err, "Failed to shut down meter provider");
    }
}

/// Builds an OTLP-enabled meter provider for the given endpoint.
fn build_otlp_meter_provider(endpoint: &str) -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
        .expect("Failed to build OTLP metric exporter");

    SdkMeterProvider::builder().with_periodic_exporter(exporter).build()
}
