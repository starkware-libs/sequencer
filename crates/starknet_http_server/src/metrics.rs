use starknet_sequencer_metrics::metrics::MetricCounter;
use tracing::info;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

// Proposal metrics.
pub(crate) const ADDED_TRANSACTIONS_TOTAL: MetricCounter =
    MetricCounter::new("ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0);
pub(crate) const ADDED_TRANSACTIONS_SUCCESS: MetricCounter = MetricCounter::new(
    "ADDED_TRANSACTIONS_SUCCESS",
    "Number of successfully added transactions",
    0,
);
pub(crate) const ADDED_TRANSACTIONS_FAILURE: MetricCounter =
    MetricCounter::new("ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0);

pub(crate) fn init_metrics() {
    info!("Initializing HTTP Server metrics");

    ADDED_TRANSACTIONS_TOTAL.register();
    ADDED_TRANSACTIONS_SUCCESS.register();
    ADDED_TRANSACTIONS_FAILURE.register();
}

// TODO(Tsabary): call the inner fn directly.
pub(crate) fn record_added_transaction() {
    ADDED_TRANSACTIONS_TOTAL.increment(1);
}

pub(crate) fn record_added_transaction_status(add_tx_success: bool) {
    if add_tx_success {
        ADDED_TRANSACTIONS_SUCCESS.increment(1);
    } else {
        ADDED_TRANSACTIONS_FAILURE.increment(1);
    }
}
