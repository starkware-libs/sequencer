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
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tower::{Layer, Service};

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_test;

pub const METRICS_PATH: &str = "/metrics";

/// How often [`spawn_upkeep`] drains the recorder's histogram samples. The
/// exporter reclaims samples only during upkeep or while rendering a scrape.
/// Without this loop, a deployment that is never scraped holds every sample
/// for the life of the process.
const UPKEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Metric name constants, so `metrics!` calls elsewhere point at one
/// definition instead of repeating string literals.
pub mod names {
    /// Build identity. Always 1, labelled with `version` and `git_sha`.
    pub const BUILD_INFO: &str = "prover_build_info";
}

/// Initializes the global Prometheus exporter and emits the `build_info`
/// gauge. Returns the handle that [`MetricsLayer`] uses to render the scrape
/// response.
///
/// Call it exactly once at startup. The handle wraps an `Arc`, so cloning it
/// is cheap.
pub fn install_exporter(version: &str, git_sha: &str) -> anyhow::Result<PrometheusHandle> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .context("Failed to install Prometheus recorder")?;
    metrics::gauge!(
        names::BUILD_INFO,
        "version" => version.to_string(),
        "git_sha" => git_sha.to_string(),
    )
    .set(1.0);
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
