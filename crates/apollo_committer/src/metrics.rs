use apollo_committer_types::communication::COMMITTER_REQUEST_LABELS;
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_metrics::{define_infra_metrics, define_metrics};
use starknet_api::block::BlockNumber;

define_infra_metrics!(committer);

define_metrics!(
    Committer => {
        MetricGauge {
            OFFSET,
            "offset",
            "The next block number to commit"
        },
        MetricGauge {
            COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
            "count_storage_tries_modifications_per_block",
            "Number of all storage tries modifications"
        },
        MetricGauge {
            COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
            "count_contracts_trie_modifications_per_block",
            "Number of contracts trie modifications"
        },
        MetricGauge {
            COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
            "count_classes_trie_modifications_per_block",
            "Number of classes trie modifications"
        },
        MetricGaugeHistogram {
            READ_DURATION_PER_BLOCK,
            "read_duration_per_block",
            "Duration of the read operation per block in seconds"
        },
        MetricGauge {
            READ_DB_ENTRIES_PER_BLOCK,
            "read_db_entries_per_block",
            "Number of read db entries per block"
        },
        MetricGaugeHistogram {
            WRITE_DURATION_PER_BLOCK,
            "write_duration_per_block",
            "Duration of the write operation per block in seconds"
        },
        MetricGauge {
            WRITE_DB_ENTRIES_PER_BLOCK,
            "write_db_entries_per_block",
            "Number of write db entries per block"
        },
        MetricGaugeHistogram {
            COMPUTE_DURATION_PER_BLOCK,
            "compute_duration_per_block",
            "Duration of the compute operation per block in seconds"
        },
    },
);

pub fn register_metrics(offset: BlockNumber) {
    OFFSET.register();
    OFFSET.set_lossy(offset.0);
    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK.register();
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK.register();
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK.register();
    READ_DURATION_PER_BLOCK.register();
    READ_DB_ENTRIES_PER_BLOCK.register();
    WRITE_DURATION_PER_BLOCK.register();
    WRITE_DB_ENTRIES_PER_BLOCK.register();
    COMPUTE_DURATION_PER_BLOCK.register();
}
