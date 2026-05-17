//! Prometheus metrics surface for the prover.
//!
//! Mirrors the proof-interceptor design: a `/metrics` endpoint exposed on the
//! same TCP listener as JSON-RPC so a single Datadog agent scrape job picks up
//! both. Implemented as a tower middleware layer that short-circuits
//! `GET /metrics` ahead of jsonrpsee, the same way `HealthLayer` handles
//! `/health`.
//!
//! Metric names follow the existing sequencer pattern (snake_case, service
//! prefix). No labels carry user-controlled or unbounded values; cardinality
//! is bounded by the small enumerations declared below.

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
    /// Requests rejected because the concurrency semaphore was full.
    pub const CONCURRENCY_REJECTED_TOTAL: &str = "prover_concurrency_rejected_total";
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
    // Pre-register the counter at 0 so it shows up in scrapes before the
    // first rejection — dashboards relying on `rate(...) > 0` need the
    // series to exist.
    metrics::counter!(names::CONCURRENCY_REJECTED_TOTAL).increment(0);
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
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
