//! Prometheus `/metrics` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /metrics` ahead of jsonrpsee so scrapes never run
//! through the JSON-RPC parser. The metric names live in [`names`].

use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::Context as _;
use bytes::Bytes;
use futures::future::{ready, Either, Ready};
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tower::{Layer, Service};

use crate::server::http_metrics;

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_test;

pub const METRICS_PATH: &str = "/metrics";

const UPKEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Bucket bounds, in seconds, for the proving-path duration histograms.
const PROVING_DURATION_BUCKETS: &[f64] =
    &[0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 7.5, 10.0, 15.0, 30.0, 60.0];

/// Explicit buckets prevent the exporter from using summaries.
const DURATION_HISTOGRAM_BUCKETS: &[(&str, &[f64])] = &[
    (names::PROVE_TRANSACTION_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (names::OS_RUN_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (names::STWO_PROVE_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (http_metrics::names::REQUEST_DURATION_SECONDS, HTTP_DURATION_BUCKETS),
];

/// Bucket bounds, in seconds, for the HTTP latency histogram. The layers above
/// short-circuit probe and scrape traffic, so these buckets cover JSON-RPC
/// calls. The range spans a millisecond-scale reject at one end and, at the
/// other, a proving POST held open for its queue wait plus the proof itself.
/// Boundaries at 2s and 10s match the proving histogram so a dashboard can read
/// the two against each other, and the 30s boundary is the default queue-wait
/// timeout.
const HTTP_DURATION_BUCKETS: &[f64] =
    &[0.005, 0.025, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0, 45.0, 60.0];

/// Metric name constants, so `metrics!` calls elsewhere point at one
/// definition instead of repeating string literals.
pub mod names {
    /// Build identity. Always 1, labelled with `version` and `git_sha`.
    pub const BUILD_INFO: &str = "prover_build_info";
    /// Unhandled panics recorded by the global panic hook.
    pub const PANICS_TOTAL: &str = "prover_panics_total";
    /// Total proving-call duration in seconds, including validation and failed calls.
    pub const PROVE_TRANSACTION_DURATION_SECONDS: &str =
        "prover_prove_transaction_duration_seconds";
    /// Completed proving calls by [`super::outcomes`] category.
    pub const PROVE_TRANSACTION_OUTCOME_TOTAL: &str = "prover_prove_transaction_outcome_total";
    /// Successful virtual OS run duration in seconds.
    pub const OS_RUN_DURATION_SECONDS: &str = "prover_os_run_duration_seconds";
    /// Successful STWO proving duration in seconds.
    pub const STWO_PROVE_DURATION_SECONDS: &str = "prover_stwo_prove_duration_seconds";
}

/// Fixed values for the `outcome` label.
pub mod outcomes {
    pub const SUCCESS: &str = "success";
    pub const FAILURE_VALIDATION: &str = "failure_validation";
    pub const FAILURE_BLOCKED: &str = "failure_blocked";
    pub const FAILURE_RUNNER: &str = "failure_runner";
    pub const FAILURE_OUTPUT_PARSE: &str = "failure_output_parse";
    pub const FAILURE_PROVING: &str = "failure_proving";
}

/// Initializes the global Prometheus exporter and emits the `build_info`
/// gauge. Returns the handle that [`MetricsLayer`] uses to render the scrape
/// response.
///
/// Call it exactly once at startup.
pub fn install_exporter(version: &str, git_sha: &str) -> anyhow::Result<PrometheusHandle> {
    let handle = configured_builder()?
        .install_recorder()
        .context("Failed to install Prometheus recorder")?;
    metrics::gauge!(
        names::BUILD_INFO,
        "version" => version.to_string(),
        "git_sha" => git_sha.to_string(),
    )
    .set(1.0);
    // Pre-register at zero so the series exists in scrapes before the first panic.
    metrics::counter!(names::PANICS_TOTAL).increment(0);
    super::http_metrics::preregister_http_metrics();
    Ok(handle)
}

/// Drains histogram samples even when nobody scrapes the endpoint.
pub fn spawn_upkeep(handle: PrometheusHandle) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(UPKEEP_INTERVAL);
        loop {
            ticker.tick().await;
            handle.run_upkeep();
        }
    })
}

pub(crate) fn configured_builder() -> anyhow::Result<PrometheusBuilder> {
    let mut builder = PrometheusBuilder::new();
    for (metric, buckets) in DURATION_HISTOGRAM_BUCKETS {
        builder = builder
            .set_buckets_for_metric(Matcher::Full((*metric).to_owned()), buckets)
            .context(format!("Failed to configure histogram buckets for {metric}"))?;
    }
    Ok(builder)
}

/// Increments a gauge on construction and decrements it on drop, so a panic or
/// a dropped future cannot leak the gauge upward.
pub struct GaugeGuard {
    metric: &'static str,
}

impl GaugeGuard {
    pub fn acquire(metric: &'static str) -> Self {
        metrics::gauge!(metric).increment(1.0);
        Self { metric }
    }
}

impl Drop for GaugeGuard {
    fn drop(&mut self) {
        metrics::gauge!(self.metric).decrement(1.0);
    }
}

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
