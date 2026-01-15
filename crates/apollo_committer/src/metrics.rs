use apollo_committer_types::communication::COMMITTER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::{define_infra_metrics, define_metrics};

// TODO(Yoav): Add the committer metrics and panels.
define_infra_metrics!(committer);

define_metrics!(
    Committer => {
        MetricHistogram {
            READ_DURATION_PER_BLOCK,
            "read_duration_per_block",
            "Duration of the read operation per block in milliseconds"
        },
        MetricHistogram {
            READ_FACTS_PER_BLOCK,
            "read_facts_per_block",
            "Number of read facts per block"
        },
        MetricHistogram {
            WRITE_DURATION_PER_BLOCK,
            "write_duration_per_block",
            "Duration of the write operation per block in milliseconds"
        },
        MetricHistogram {
            NEW_FACTS_PER_BLOCK,
            "new_facts_per_block",
            "Number of new facts per block"
        },
        MetricHistogram {
            COMPUTE_DURATION_PER_BLOCK,
            "compute_duration_per_block",
            "Duration of the compute operation per block in milliseconds"
        },
    },
);

pub fn register_metrics() {
    READ_DURATION_PER_BLOCK.register();
    READ_FACTS_PER_BLOCK.register();
    WRITE_DURATION_PER_BLOCK.register();
    NEW_FACTS_PER_BLOCK.register();
    COMPUTE_DURATION_PER_BLOCK.register();
}
