use apollo_metrics::define_metrics;

define_metrics!(
    Storage => {
        MetricHistogram { STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, "storage_append_thin_state_diff_latency_seconds", "Latency to append thin state diff in storage (secs)" },
        MetricHistogram { STORAGE_COMMIT_LATENCY, "storage_commit_latency_seconds", "Latency to commit changes in storage (secs)" },
        MetricGauge { STORAGE_OPEN_SYNC_READ_TRANSACTIONS, "storage_open_sync_read_transactions", "The number of open sync  read transactions" },
        MetricGauge { STORAGE_OPEN_BATCHER_READ_TRANSACTIONS, "storage_open_batcher_read_transactions", "The number of open batcher read transactions" },
        MetricGauge { STORAGE_OPEN_CLASS_MANAGER_READ_TRANSACTIONS, "storage_open_class_manager_read_transactions", "The number of open class manager read transactions" },
    },
);

pub(crate) fn register_metrics() {
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY.register();
    STORAGE_COMMIT_LATENCY.register();
    STORAGE_OPEN_SYNC_READ_TRANSACTIONS.register();
    STORAGE_OPEN_BATCHER_READ_TRANSACTIONS.register();
    STORAGE_OPEN_CLASS_MANAGER_READ_TRANSACTIONS.register();
}
