use apollo_metrics::define_metrics;
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
