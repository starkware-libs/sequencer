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
