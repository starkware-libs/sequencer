//! tower middleware that records HTTP-level Prometheus metrics:
//! request count, latency histogram, and an RAII-guarded in-flight gauge.

use std::task::{Context, Poll};
use std::time::Instant;

use http::{Method, Request, Response, StatusCode};
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};

#[cfg(test)]
#[path = "http_metrics_test.rs"]
mod http_metrics_test;

pub mod names {
    /// Counter of HTTP requests by method + status code.
    pub const REQUESTS_TOTAL: &str = "prover_http_requests_total";
    /// Histogram of end-to-end HTTP request latency by method.
    pub const REQUEST_DURATION_SECONDS: &str = "prover_http_request_duration_seconds";
    /// Gauge of in-flight HTTP requests.
    pub const IN_FLIGHT_REQUESTS: &str = "prover_http_inflight_requests";
}

/// Pre-registers the HTTP metrics so the series exist before the first request.
/// The histogram is only described, not pre-`record`ed: a phantom 0-second
/// observation would distort every quantile.
pub fn preregister_http_metrics() {
    // Use the same label helpers as the live path so pre-registered series match the emitted
    // vocabulary.
    metrics::counter!(
        names::REQUESTS_TOTAL,
        "method" => method_label(&Method::POST),
        "status" => status_label(StatusCode::OK),
    )
    .increment(0);
    metrics::describe_histogram!(
        names::REQUEST_DURATION_SECONDS,
        "HTTP request latency in seconds, by method",
    );
    metrics::gauge!(names::IN_FLIGHT_REQUESTS).set(0.0);
}

#[derive(Clone, Copy)]
pub struct HttpMetricsLayer;

impl<S> Layer<S> for HttpMetricsLayer {
    type Service = HttpMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpMetricsService { inner }
    }
}

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
            metrics::gauge!(names::IN_FLIGHT_REQUESTS).increment(1.0);
            let _in_flight_guard = InFlightGuard;
            let result = future.await;
            let duration_seconds = start.elapsed().as_secs_f64();
            let status = match &result {
                Ok(response) => status_label(response.status()),
                // Sentinel: tower stack failure, no HTTP response produced.
                Err(_) => "error",
            };
            metrics::histogram!(names::REQUEST_DURATION_SECONDS, "method" => method)
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

/// Decrements the in-flight gauge on drop, so panic and cancellation paths
/// can't leak the gauge upward.
struct InFlightGuard;

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        metrics::gauge!(names::IN_FLIGHT_REQUESTS).decrement(1.0);
    }
}
