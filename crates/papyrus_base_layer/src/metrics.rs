use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricCounter;
use tracing::info;

define_metrics!(
    BaseLayer => {
        MetricCounter { BASE_LAYER_REQUESTS_TOTAL, "base_layer_requests_total", "Total number of requests made to the base layer contract", init = 0 },
    },
);

pub(crate) fn init_metrics() {
    info!("Initializing Base Layer metrics");
    BASE_LAYER_REQUESTS_TOTAL.register();
}
