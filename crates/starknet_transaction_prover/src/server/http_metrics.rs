//! tower middleware that records HTTP-level Prometheus metrics:
//! request count, latency histogram, and an RAII-guarded in-flight gauge.

use std::task::{Context, Poll};
use std::time::Instant;

use http::{Method, Request, Response, StatusCode};
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};

use crate::server::metrics::GaugeGuard;

#[cfg(test)]
#[path = "http_metrics_test.rs"]
mod http_metrics_test;

pub mod names {
    /// Counter of HTTP requests by method + status class.
    pub const REQUESTS_TOTAL: &str = "prover_http_requests_total";
    /// Histogram of end-to-end HTTP request latency by method + status class.
    pub const REQUEST_DURATION_SECONDS: &str = "prover_http_request_duration_seconds";
    /// Gauge of in-flight HTTP requests.
    pub const IN_FLIGHT_REQUESTS: &str = "prover_http_inflight_requests";
}

/// Pre-registers the HTTP metrics so the series exist before the first request.
/// This describes the histogram without recording into it. A phantom 0-second
/// observation would distort every quantile.
pub fn preregister_http_metrics() {
    // Pre-register the 2xx/4xx/5xx series so an error-rate alert reads zero instead of an
    // absent series before the first failure. This calls the same label helpers as the live
    // path, so the pre-registered label values match the emitted ones.
    for status in [StatusCode::OK, StatusCode::BAD_REQUEST, StatusCode::INTERNAL_SERVER_ERROR] {
        metrics::counter!(
            names::REQUESTS_TOTAL,
            "method" => method_label(&Method::POST),
            "status" => status_label(status),
        )
        .increment(0);
    }
    metrics::describe_histogram!(
        names::REQUEST_DURATION_SECONDS,
        "HTTP request latency in seconds, by method and status class",
    );
    metrics::gauge!(names::IN_FLIGHT_REQUESTS).set(0.0);
}

/// tower [`Layer`] that records request count, latency, and the in-flight gauge
/// for served requests.
#[derive(Clone, Copy)]
pub struct HttpMetricsLayer;

impl<S> Layer<S> for HttpMetricsLayer {
    type Service = HttpMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpMetricsService { inner }
    }
}

/// tower [`Service`] produced by [`HttpMetricsLayer`].
#[derive(Clone)]
pub struct HttpMetricsService<S> {
    inner: S,
}

impl<S, ReqB> Service<Request<ReqB>> for HttpMetricsService<S>
where
    S: Service<Request<ReqB>, Response = Response<HttpBody>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<HttpBody>;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqB>) -> Self::Future {
        let method = method_label(request.method());
        let start = Instant::now();
        let future = self.inner.call(request);

        Box::pin(async move {
            let _in_flight_guard = GaugeGuard::acquire(names::IN_FLIGHT_REQUESTS);
            let result = future.await;
            let duration_seconds = start.elapsed().as_secs_f64();
            let status = match &result {
                Ok(response) => status_label(response.status()),
                // The tower stack failed without producing an HTTP response.
                Err(_) => "error",
            };
            metrics::histogram!(
                names::REQUEST_DURATION_SECONDS,
                "method" => method,
                "status" => status,
            )
            .record(duration_seconds);
            metrics::counter!(
                names::REQUESTS_TOTAL,
                "method" => method,
                "status" => status,
            )
            .increment(1);
            result
        })
    }
}

/// Collapses HTTP statuses into a bounded set of label values to cap Prometheus series cardinality.
fn status_label(status: StatusCode) -> &'static str {
    match status.as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

/// Collapses HTTP methods into a bounded set of label values to cap Prometheus series cardinality.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::PATCH => "PATCH",
        _ => "other",
    }
}
