//! tower middleware that records HTTP-level Prometheus metrics.
//!
//! Sister layer to `RequestLogLayer`: where the latter emits one log line
//! per request, this one feeds three Prometheus series so dashboards can
//! query request rate, latency, and concurrency without parsing logs.
//!
//! Sits outermost in the tower stack so the timings include every other
//! layer. Cardinality is bounded:
//! - `method` — the small enumeration of HTTP methods the server actually accepts (POST in
//!   practice; GET for /health and /metrics).
//! - `status` — HTTP status code as a string. Always one of a handful of values because the inner
//!   jsonrpsee/tower stack normalizes errors.

use std::task::{Context, Poll};
use std::time::Instant;

use http::{Request, Response};
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};

#[cfg(test)]
#[path = "http_metrics_test.rs"]
mod http_metrics_test;

/// Metric name constants.
pub mod names {
    /// Counter of HTTP requests by method + status code.
    pub const REQUESTS_TOTAL: &str = "prover_http_requests_total";
    /// Histogram of end-to-end HTTP request latency by method.
    pub const REQUEST_DURATION_SECONDS: &str = "prover_http_request_duration_seconds";
    /// Gauge of in-flight HTTP requests.
    pub const IN_FLIGHT_REQUESTS: &str = "prover_http_inflight_requests";
}

/// Pre-registers the three HTTP metrics at zero so they appear in /metrics
/// even before the first request — dashboards relying on `rate(...) > 0`
/// need the series to exist. Should be called once at startup, alongside
/// [`super::metrics::install_exporter`].
pub fn preregister_http_metrics() {
    metrics::counter!(names::REQUESTS_TOTAL, "method" => "POST", "status" => "200").increment(0);
    metrics::histogram!(names::REQUEST_DURATION_SECONDS, "method" => "POST").record(0.0);
    metrics::gauge!(names::IN_FLIGHT_REQUESTS).set(0.0);
}

/// tower [`Layer`] producing [`HttpMetricsService`].
#[derive(Clone, Copy, Default)]
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
        let method = request.method().as_str().to_string();
        let start = Instant::now();
        let future = self.inner.call(request);

        Box::pin(async move {
            metrics::gauge!(names::IN_FLIGHT_REQUESTS).increment(1.0);
            // Guard ensures decrement runs even if the inner future panics
            // or is cancelled.
            let _in_flight_guard = InFlightGuard;
            let result = future.await;
            let duration_seconds = start.elapsed().as_secs_f64();
            let status_code = match &result {
                Ok(response) => response.status().as_u16(),
                // Inner service error: use 0 so dashboards can filter on
                // it as a sentinel for "tower stack failure, no HTTP
                // response was produced".
                Err(_) => 0,
            };
            let status_label = status_code.to_string();
            metrics::histogram!(names::REQUEST_DURATION_SECONDS, "method" => method.clone())
                .record(duration_seconds);
            metrics::counter!(
                names::REQUESTS_TOTAL,
                "method" => method,
                "status" => status_label,
            )
            .increment(1);
            result
        })
    }
}

/// Decrements the in-flight gauge when dropped. Using a guard rather than
/// an explicit decrement after `future.await` covers panic + cancellation
/// paths so the gauge can't leak upward without coming back down.
struct InFlightGuard;

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        metrics::gauge!(names::IN_FLIGHT_REQUESTS).decrement(1.0);
    }
}
