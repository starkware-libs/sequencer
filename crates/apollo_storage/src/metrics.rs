use apollo_metrics::define_metrics;

define_metrics!(
    Storage => {
        MetricHistogram { STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, "storage_append_thin_state_diff_latency_seconds", "Latency to append thin state diff in storage (secs)" },
        MetricHistogram { STORAGE_COMMIT_LATENCY, "storage_commit_latency_seconds", "Latency to commit changes in storage (secs)" },
        MetricGauge { SYNC_STORAGE_OPEN_READ_TRANSACTIONS, "sync_storage_open_read_transactions", "The number of open sync  read transactions" },
        MetricGauge { BATCHER_STORAGE_OPEN_READ_TRANSACTIONS, "batcher_storage_open_read_transactions", "The number of open batcher read transactions" },
        MetricGauge { CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS, "class_manager_storage_open_read_transactions", "The number of open class manager read transactions" },
    },
);

pub(crate) fn register_metrics() {
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY.register();
    STORAGE_COMMIT_LATENCY.register();
    SYNC_STORAGE_OPEN_READ_TRANSACTIONS.register();
    BATCHER_STORAGE_OPEN_READ_TRANSACTIONS.register();
    CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS.register();
}
