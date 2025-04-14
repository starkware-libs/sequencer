use apollo_metrics::define_metrics;
use apollo_metrics::metrics::{MetricGauge, MetricHistogram};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in a valid proposal received" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in a valid proposal received" },
        MetricGauge { CONSENSUS_L2_GAS_PRICE, "consensus_l2_gas_price", "The L2 gas price calculated in a valid proposal received" },

        // Cende metrics
        MetricGauge { CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER, "cende_last_prepared_blob_block_number", "The blob block number that cende knows. That means the sequencer can be the proposer only if the current height is greater by one than this value." },
        MetricHistogram { CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY, "cende_prepare_blob_for_next_height_latency", "The time it takes to prepare the blob for the next height, i.e create the blob object." },
        // TODO(dvir): consider to differ the case when the blob was already written, that will prevent using the `sequencer_latency_histogram` attribute.
        // TODO(dvir): add a counter for successful blob writes and failed blob writes.
        MetricHistogram { CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY, "cende_write_prev_height_blob_latency", "Be careful with this metric, if the blob was already written by another request, the latency is much lower since wirting to Aerospike is not needed." },
    }
);

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.register();
    CONSENSUS_NUM_TXS_IN_PROPOSAL.register();
    CONSENSUS_L2_GAS_PRICE.register();
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.register();
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.register();
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.register();
}
