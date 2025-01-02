use metrics::{absolute_counter, describe_counter, register_counter};
use tracing::info;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

pub(crate) const ADDED_TRANSACTIONS_TOTAL: (&str, &str, u64) =
    ("ADDED_TRANSACTIONS_TOTAL", "Total number of transactions added", 0);
pub(crate) const ADDED_TRANSACTIONS_SUCCESS: (&str, &str, u64) =
    ("ADDED_TRANSACTIONS_SUCCESS", "Number of successfully added transactions", 0);
pub(crate) const ADDED_TRANSACTIONS_FAILURE: (&str, &str, u64) =
    ("ADDED_TRANSACTIONS_FAILURE", "Number of faulty added transactions", 0);

pub(crate) fn init_metrics() {
    info!("Initializing HTTP Server metrics");
    register_counter!(ADDED_TRANSACTIONS_TOTAL.0);
    describe_counter!(ADDED_TRANSACTIONS_TOTAL.0, ADDED_TRANSACTIONS_TOTAL.1);
    absolute_counter!(ADDED_TRANSACTIONS_TOTAL.0, ADDED_TRANSACTIONS_TOTAL.2);

    register_counter!(ADDED_TRANSACTIONS_SUCCESS.0);
    describe_counter!(ADDED_TRANSACTIONS_SUCCESS.0, ADDED_TRANSACTIONS_SUCCESS.1);
    absolute_counter!(ADDED_TRANSACTIONS_SUCCESS.0, ADDED_TRANSACTIONS_SUCCESS.2);

    register_counter!(ADDED_TRANSACTIONS_FAILURE.0);
    describe_counter!(ADDED_TRANSACTIONS_FAILURE.0, ADDED_TRANSACTIONS_FAILURE.1);
    absolute_counter!(ADDED_TRANSACTIONS_FAILURE.0, ADDED_TRANSACTIONS_FAILURE.2);
}

pub(crate) fn record_added_transaction() {
    metrics::increment_counter!(ADDED_TRANSACTIONS_TOTAL.0);
}

pub(crate) fn record_added_transaction_status(add_tx_success: bool) {
    if add_tx_success {
        metrics::increment_counter!(ADDED_TRANSACTIONS_SUCCESS.0);
    } else {
        metrics::increment_counter!(ADDED_TRANSACTIONS_FAILURE.0);
    }
}
