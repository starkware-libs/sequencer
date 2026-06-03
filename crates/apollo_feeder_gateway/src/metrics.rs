use apollo_metrics::define_metrics;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tracing::info;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

define_metrics!(
    FeederGateway => {
        MetricCounter { FEEDER_GATEWAY_REQUESTS_TOTAL, "feeder_gateway_requests_total", "Total number of feeder gateway requests", init = 0 },
    },
);

pub(crate) fn init_metrics() {
    info!("Initializing FeederGateway metrics");
    FEEDER_GATEWAY_REQUESTS_TOTAL.register();
}

/// Axum middleware recording the request metric. Applied once as a router layer over the API
/// routes, rather than copied into each handler.
pub(crate) async fn record_request_metrics(request: Request, next: Next) -> Response {
    FEEDER_GATEWAY_REQUESTS_TOTAL.increment(1);
    next.run(request).await
}
