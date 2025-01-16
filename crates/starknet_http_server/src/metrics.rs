use metrics::{counter, describe_counter};
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
    counter!(ADDED_TRANSACTIONS_TOTAL.0).absolute(ADDED_TRANSACTIONS_TOTAL.2);
    describe_counter!(ADDED_TRANSACTIONS_TOTAL.0, ADDED_TRANSACTIONS_TOTAL.1);

    counter!(ADDED_TRANSACTIONS_SUCCESS.0).absolute(ADDED_TRANSACTIONS_SUCCESS.2);
    describe_counter!(ADDED_TRANSACTIONS_SUCCESS.0, ADDED_TRANSACTIONS_SUCCESS.1);

    counter!(ADDED_TRANSACTIONS_FAILURE.0).absolute(ADDED_TRANSACTIONS_FAILURE.2);
    describe_counter!(ADDED_TRANSACTIONS_FAILURE.0, ADDED_TRANSACTIONS_FAILURE.1);
}

pub(crate) fn record_added_transaction() {
    counter!(ADDED_TRANSACTIONS_TOTAL.0).increment(1);
}

pub(crate) fn record_added_transaction_status(add_tx_success: bool) {
    if add_tx_success {
        counter!(ADDED_TRANSACTIONS_SUCCESS.0).increment(1);
    } else {
        counter!(ADDED_TRANSACTIONS_FAILURE.0).increment(1);
    }
}
