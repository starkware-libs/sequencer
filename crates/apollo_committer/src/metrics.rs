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
            READ_DURATION_PER_BLOCK_HIST,
            "read_duration_per_block_hist",
            "Duration of the read operation per block in milliseconds"
        },
        MetricGauge {
            READ_DURATION_PER_BLOCK,
            "read_duration_per_block",
            "Duration of the read operation per block in milliseconds"
        },
        MetricGauge {
            READ_DB_ENTRIES_PER_BLOCK,
            "read_db_entries_per_block",
            "Number of read db entries per block"
        },
        MetricHistogram {
            WRITE_DURATION_PER_BLOCK_HIST,
            "write_duration_per_block",
            "Duration of the write operation per block in milliseconds"
        },
        MetricGauge {
            WRITE_DURATION_PER_BLOCK,
            "write_duration_per_block",
            "Duration of the write operation per block in milliseconds"
        },
        MetricGauge {
            WRITE_DB_ENTRIES_PER_BLOCK,
            "write_db_entries_per_block",
            "Number of write db entries per block"
        },
        MetricHistogram {
            COMPUTE_DURATION_PER_BLOCK_HIST,
            "compute_duration_per_block",
            "Duration of the compute operation per block in milliseconds"
        },
        MetricGauge {
            COMPUTE_DURATION_PER_BLOCK,
            "compute_duration_per_block",
            "Duration of the compute operation per block in milliseconds"
        },
    },
);

pub fn register_metrics() {
    READ_DURATION_PER_BLOCK_HIST.register();
    READ_DURATION_PER_BLOCK.register();
    READ_DB_ENTRIES_PER_BLOCK.register();
    WRITE_DURATION_PER_BLOCK_HIST.register();
    WRITE_DURATION_PER_BLOCK.register();
    WRITE_DB_ENTRIES_PER_BLOCK.register();
    COMPUTE_DURATION_PER_BLOCK_HIST.register();
    COMPUTE_DURATION_PER_BLOCK.register();
}
