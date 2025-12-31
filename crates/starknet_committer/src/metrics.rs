use apollo_metrics::define_metrics;

// TODO(Rotem): add metrics when the apollo_committer is ready:
// - COMPUTE_DURATION_PER_BLOCK
// - WRITE_DURATION_PER_BLOCK
// - NEW_FACTS_PER_BLOCK
define_metrics!(
    StarknetCommitter => {
        MetricGauge {
            READ_DURATION_PER_BLOCK,
            "read_duration_per_block",
            "The duration of the read operation per block in milliseconds"
        },
        MetricGauge {
            READ_FACTS_PER_BLOCK,
            "read_facts_per_block",
            "The number of read facts per block"
        },
    },
);

// TODO(Rotem): call this function from the apollo_committer initialization when it's ready.
#[allow(dead_code)]
pub(crate) fn register_metrics() {
    READ_DURATION_PER_BLOCK.register();
    READ_FACTS_PER_BLOCK.register();
}
