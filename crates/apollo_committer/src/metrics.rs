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
            COMMITTER_OFFSET,
            "committer_offset",
            "The next block number to commit"
        },
        MetricCounter {
            BLOCKS_COMMITTED,
            "blocks_committed",
            "Number of blocks committed, in commit and revert",
            init = 0
        },
        MetricHistogram {
            COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
            "count_storage_tries_modifications_per_block",
            "Number of all storage tries modifications"
        },
        MetricHistogram {
            COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
            "count_contracts_trie_modifications_per_block",
            "Number of contracts trie modifications"
        },
        MetricHistogram {
            COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
            "count_classes_trie_modifications_per_block",
            "Number of classes trie modifications"
        },
        MetricHistogram {
            COUNT_EMPTIED_LEAVES_PER_BLOCK,
            "count_emptied_leaves_per_block",
            "Number of leaves emptied in the storage tries per block"
        },
        MetricHistogram {
            EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK,
            "emptied_leaves_percentage_per_block",
            "Percentage of storage tries leaves emptied over the total number of storage tries leaves per block"
        },
        MetricCounter {
            TOTAL_BLOCK_DURATION,
            "total_block_duration",
            "Total block commit duration in milliseconds (cumulative)",
            init = 0
        },
        MetricCounter {
            TOTAL_BLOCK_DURATION_PER_MODIFICATION,
            "total_block_duration_per_modification",
            "Duration of the block commit normalized by the number of modifications in microseconds (cumulative)",
            init = 0
        },
        MetricCounter {
            READ_DURATION_PER_BLOCK,
            "read_duration_per_block",
            "Duration of the read operation per block in milliseconds (cumulative)",
            init = 0
        },
        MetricHistogram {
            AVERAGE_READ_RATE,
            "average_read_rate",
            "Reads per second average over a block"
        },
        MetricCounter {
            WRITE_DURATION_PER_BLOCK,
            "write_duration_per_block",
            "Duration of the write operation per block in milliseconds (cumulative)",
            init = 0
        },
        MetricHistogram {
            AVERAGE_WRITE_RATE,
            "average_write_rate",
            "Writes per second average over a block"
        },
        MetricCounter {
            COMPUTE_DURATION_PER_BLOCK,
            "compute_duration_per_block",
            "Duration of the compute operation per block in milliseconds (cumulative)",
            init = 0
        },
        MetricHistogram {
            AVERAGE_COMPUTE_RATE,
            "average_compute_rate",
            "Compute written entries per second average over a block"
        },
    },
);

pub fn register_metrics(offset: BlockNumber) {
    COMMITTER_OFFSET.register();
    COMMITTER_OFFSET.set_lossy(offset.0);
    BLOCKS_COMMITTED.register();
    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK.register();
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK.register();
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK.register();
    COUNT_EMPTIED_LEAVES_PER_BLOCK.register();
    EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK.register();
    TOTAL_BLOCK_DURATION.register();
    TOTAL_BLOCK_DURATION_PER_MODIFICATION.register();
    READ_DURATION_PER_BLOCK.register();
    AVERAGE_READ_RATE.register();
    WRITE_DURATION_PER_BLOCK.register();
    AVERAGE_WRITE_RATE.register();
    COMPUTE_DURATION_PER_BLOCK.register();
}
