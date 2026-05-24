//! Prometheus `/metrics` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /metrics` ahead of jsonrpsee so scrapes never run
//! through the JSON-RPC parser. Label cardinality is bounded by the
//! enumerations in [`names`] — no user-controlled values become labels.

use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::{ready, Either, Ready};
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tower::{Layer, Service};

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_test;

/// Path served by [`MetricsLayer`].
pub const METRICS_PATH: &str = "/metrics";

/// Metric name constants. Kept here so `metrics!` invocations elsewhere link
/// to a single definition instead of bare string literals.
pub mod names {
    /// Build identity. Value is always 1; labels carry version + git_sha.
    pub const BUILD_INFO: &str = "prover_build_info";
    /// Wall-clock duration of `prove_transaction` end-to-end. Bucketed.
    pub const PROVE_TRANSACTION_DURATION_SECONDS: &str =
        "prover_prove_transaction_duration_seconds";
    /// `prove_transaction` outcomes by category. See [`super::outcomes`] for
    /// the fixed set of label values.
    pub const PROVE_TRANSACTION_OUTCOME_TOTAL: &str = "prover_prove_transaction_outcome_total";
    /// Virtual SNOS run sub-step duration. Bucketed.
    pub const OS_RUN_DURATION_SECONDS: &str = "prover_os_run_duration_seconds";
    /// Stwo proving sub-step duration. Bucketed.
    pub const STWO_PROVE_DURATION_SECONDS: &str = "prover_stwo_prove_duration_seconds";
}

/// Fixed, bounded set of values for the `outcome` label on
/// [`names::PROVE_TRANSACTION_OUTCOME_TOTAL`]. Adding a variant requires a
/// dashboard update.
pub mod outcomes {
    pub const SUCCESS: &str = "success";
    pub const VALIDATION: &str = "failure_validation";
    pub const BLOCKED: &str = "failure_blocked";
    pub const RUNNER: &str = "failure_runner";
    pub const OUTPUT_PARSE: &str = "failure_output_parse";
    pub const PROVING: &str = "failure_proving";
    /// Rejected at admission because the queue (running + waiting) was full.
    pub const REJECTED_QUEUE_FULL: &str = "rejected_queue_full";
    /// Rejected after waiting past `queue_wait_timeout` for a worker slot.
    pub const REJECTED_WAIT_TIMEOUT: &str = "rejected_wait_timeout";
}

/// Initializes the global Prometheus exporter and emits the `build_info`
/// gauge. Returns the handle used by [`MetricsLayer`] to render the scrape
/// response.
///
/// Should be called exactly once at startup. The handle is cheap to clone
/// (it wraps an `Arc`).
pub fn install_exporter(version: &str, git_sha: &str) -> anyhow::Result<PrometheusHandle> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|err| anyhow::anyhow!("failed to install prometheus recorder: {err}"))?;
    metrics::gauge!(
        names::BUILD_INFO,
        "version" => version.to_string(),
        "git_sha" => git_sha.to_string(),
    )
    .set(1.0);
    // Pre-register counters/gauges at zero so they show up in scrapes
    // before the first request — dashboards relying on `rate(...) > 0`
    // need the series to exist.
    super::http_metrics::preregister_http_metrics();
    Ok(handle)
}

/// tower [`Layer`] that intercepts `GET /metrics`.
#[derive(Clone)]
pub struct MetricsLayer {
    handle: PrometheusHandle,
}

impl MetricsLayer {
    pub fn new(handle: PrometheusHandle) -> Self {
        Self { handle }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService { inner, handle: self.handle.clone() }
    }
}

#[derive(Clone)]
pub struct MetricsService<S> {
    inner: S,
    handle: PrometheusHandle,
}

impl<S, ReqB> Service<Request<ReqB>> for MetricsService<S>
where
    S: Service<Request<ReqB>, Response = Response<HttpBody>>,
{
    type Response = Response<HttpBody>;
    type Error = S::Error;
    type Future = Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<ReqB>) -> Self::Future {
        if request.method() == Method::GET && request.uri().path() == METRICS_PATH {
            let body = Bytes::from(self.handle.render());
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
                .body(HttpBody::new(Full::new(body)))
                .expect("response build with a string body is infallible");
            return Either::Left(ready(Ok(response)));
        }
        Either::Right(self.inner.call(request))
    }
}
