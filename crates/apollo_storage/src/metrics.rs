use apollo_metrics::define_metrics;

define_metrics!(
    Storage => {
        MetricHistogram { STORAGE_APPEND_THIN_STATE_DIFF_LATENCY, "storage_append_thin_state_diff_latency_seconds", "Latency to append thin state diff in storage (secs)" },
        MetricHistogram { STORAGE_COMMIT_LATENCY, "storage_commit_latency_seconds", "Latency to commit changes in storage (secs)" },
    },
);

pub(crate) fn register_metrics() {
    STORAGE_APPEND_THIN_STATE_DIFF_LATENCY.register();
    STORAGE_COMMIT_LATENCY.register();
}
