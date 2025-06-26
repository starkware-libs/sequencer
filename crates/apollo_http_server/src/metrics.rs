use apollo_metrics::define_metrics;
use tracing::info;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

// TODO(Yael): consider adding labels for different endpoints.
define_metrics!(
    HttpServer => {
        MetricCounter { ADDED_TRANSACTIONS_TOTAL, "http_server_added_transactions_total", "Total number of transactions added", init = 0 },
        MetricCounter { ADDED_TRANSACTIONS_SUCCESS, "http_server_added_transactions_success", "Number of successfully added transactions", init = 0 },
        MetricCounter { ADDED_TRANSACTIONS_FAILURE, "http_server_added_transactions_failure", "Number of faulty added transactions", init = 0 },
        MetricCounter { ADDED_TRANSACTIONS_INTERNAL_ERROR, "http_server_added_transactions_internal_error", "Number of faulty added transactions failing on internal error", init = 0 },
        MetricHistogram { HTTP_SERVER_ADD_TX_LATENCY, "http_server_add_tx_latency", "Latency of HTTP add_tx endpoint in secs" },
    },
);

pub(crate) fn init_metrics() {
    info!("Initializing HTTP Server metrics");
    ADDED_TRANSACTIONS_TOTAL.register();
    ADDED_TRANSACTIONS_SUCCESS.register();
    ADDED_TRANSACTIONS_FAILURE.register();
    ADDED_TRANSACTIONS_INTERNAL_ERROR.register();
    HTTP_SERVER_ADD_TX_LATENCY.register();
}
