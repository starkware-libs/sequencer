//! Prometheus `/metrics` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /metrics` ahead of jsonrpsee so scrapes never run
//! through the JSON-RPC parser. The metric names live in [`names`]. No
//! user-controlled value becomes a label, so cardinality stays bounded.

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

/// How often [`spawn_upkeep`] drains the recorder's histogram samples. The
/// exporter reclaims samples only during upkeep or while rendering a scrape.
/// Without this loop, a deployment that is never scraped holds every sample
/// for the life of the process.
const UPKEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Bucket bounds, in seconds, for the proving-path duration histograms. A proof
/// typically takes about 2s and is not expected to exceed roughly 10s, so the
/// resolution sits there, with boundaries landing exactly on both values.
/// Quantile queries interpolate *within* a bucket, so a boundary on the value
/// you alert on is what makes "fraction of proofs under 10s" exact rather than
/// estimated. The tail past 10s exists to make a pathological proof visible,
/// not to measure it precisely.
const PROVING_DURATION_BUCKETS: &[f64] =
    &[0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 7.5, 10.0, 15.0, 30.0, 60.0];

/// Every duration histogram, with the buckets it renders into. The exporter
/// turns a histogram with no configured bucket bounds into a Prometheus
/// *summary*, which carries pre-computed quantiles over a rolling window and no
/// `_bucket` series. `histogram_quantile()` in a dashboard query then returns
/// nothing at all.
const DURATION_HISTOGRAM_BUCKETS: &[(&str, &[f64])] = &[
    (names::PROVE_TRANSACTION_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (names::OS_RUN_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (names::STWO_PROVE_DURATION_SECONDS, PROVING_DURATION_BUCKETS),
    (http_metrics::names::REQUEST_DURATION_SECONDS, HTTP_DURATION_BUCKETS),
    (names::QUEUE_WAIT_DURATION_SECONDS, QUEUE_WAIT_DURATION_BUCKETS),
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

/// Bucket bounds, in seconds, for the queue-wait histogram. A request either
/// finds a free worker slot at once or waits behind proofs, so the buckets are
/// densest near zero. The last boundary is the default queue-wait timeout, so
/// the count of waits that ran to the timeout is exact.
const QUEUE_WAIT_DURATION_BUCKETS: &[f64] =
    &[0.001, 0.01, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0];

/// Metric name constants, so `metrics!` calls elsewhere point at one
/// definition instead of repeating string literals.
pub mod names {
    /// Build identity. Always 1, labelled with `version` and `git_sha`.
    pub const BUILD_INFO: &str = "prover_build_info";
    /// Unhandled panics recorded by the global panic hook.
    pub const PANICS_TOTAL: &str = "prover_panics_total";
    /// Wall-clock duration of the whole `prove_transaction` call (validation,
    /// blocking check, OS run, proving), labelled by `outcome` so a query can
    /// separate success latency from failure latency. Bucketed.
    pub const PROVE_TRANSACTION_DURATION_SECONDS: &str =
        "prover_prove_transaction_duration_seconds";
    /// `prove_transaction` outcomes by category. See [`super::outcomes`] for
    /// the fixed set of label values.
    pub const PROVE_TRANSACTION_OUTCOME_TOTAL: &str = "prover_prove_transaction_outcome_total";
    /// Virtual SNOS run sub-step duration. Bucketed.
    pub const OS_RUN_DURATION_SECONDS: &str = "prover_os_run_duration_seconds";
    /// Stwo proving sub-step duration. Bucketed.
    pub const STWO_PROVE_DURATION_SECONDS: &str = "prover_stwo_prove_duration_seconds";
    /// Requests admitted to the queue but not yet running (waiting for a worker slot). Gauge.
    pub const QUEUE_WAITING_REQUESTS: &str = "prover_queue_waiting_requests";
    /// Time a request waited in the queue before acquiring a worker slot. Bucketed.
    pub const QUEUE_WAIT_DURATION_SECONDS: &str = "prover_queue_wait_duration_seconds";
}

/// Fixed, bounded set of values for the `outcome` label on
/// [`names::PROVE_TRANSACTION_OUTCOME_TOTAL`].
pub mod outcomes {
    pub const SUCCESS: &str = "success";
    pub const FAILURE_VALIDATION: &str = "failure_validation";
    pub const FAILURE_BLOCKED: &str = "failure_blocked";
    pub const FAILURE_RUNNER: &str = "failure_runner";
    pub const FAILURE_OUTPUT_PARSE: &str = "failure_output_parse";
    pub const FAILURE_PROVING: &str = "failure_proving";
    /// Rejected at admission because the queue (running + waiting) was full.
    pub const REJECTED_QUEUE_FULL: &str = "rejected_queue_full";
    /// Rejected after waiting past `queue_wait_timeout` for a worker slot.
    pub const REJECTED_WAIT_TIMEOUT: &str = "rejected_wait_timeout";
}

/// Initializes the global Prometheus exporter and emits the `build_info`
/// gauge. Returns the handle that [`MetricsLayer`] uses to render the scrape
/// response.
///
/// Call it exactly once at startup. The handle wraps an `Arc`, so cloning it
/// is cheap.
pub fn install_exporter(version: &str, git_sha: &str) -> anyhow::Result<PrometheusHandle> {
    let mut builder = PrometheusBuilder::new();
    for (metric, buckets) in DURATION_HISTOGRAM_BUCKETS {
        builder = builder
            .set_buckets_for_metric(Matcher::Full((*metric).to_owned()), buckets)
            .context(format!("Failed to configure histogram buckets for {metric}"))?;
    }
    let handle = builder.install_recorder().context("Failed to install Prometheus recorder")?;
    metrics::gauge!(
        names::BUILD_INFO,
        "version" => version.to_string(),
        "git_sha" => git_sha.to_string(),
    )
    .set(1.0);
    // Pre-register at zero so the series exists in scrapes before the first panic.
    metrics::counter!(names::PANICS_TOTAL).increment(0);
    super::http_metrics::preregister_http_metrics();
    // Queue depth starts at zero. Busy-rejects are folded into the outcome counter, so
    // pre-register both reject outcomes too. A rejection-rate query then has series from startup.
    metrics::gauge!(names::QUEUE_WAITING_REQUESTS).set(0.0);
    metrics::counter!(
        names::PROVE_TRANSACTION_OUTCOME_TOTAL,
        "outcome" => outcomes::REJECTED_QUEUE_FULL,
    )
    .increment(0);
    metrics::counter!(
        names::PROVE_TRANSACTION_OUTCOME_TOTAL,
        "outcome" => outcomes::REJECTED_WAIT_TIMEOUT,
    )
    .increment(0);
    Ok(handle)
}

/// Spawns the recorder's upkeep loop. Separate from [`install_exporter`] so
/// tests can install a recorder without a tokio runtime.
pub fn spawn_upkeep(handle: PrometheusHandle) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(UPKEEP_INTERVAL);
        loop {
            ticker.tick().await;
            handle.run_upkeep();
        }
    })
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
