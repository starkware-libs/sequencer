use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{EnumIter, IntoStaticStr};

define_metrics!(
    ConsensusOrchestrator => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in a valid proposal received" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in a valid proposal received" },
        MetricCounter { CONSENSUS_L1_GAS_MISMATCH, "consensus_l1_gas_mismatch", "The number of times the L1 gas in a proposal does not match the value expected by this validator", init = 0 },
        MetricCounter { CONSENSUS_L1_DATA_GAS_MISMATCH, "consensus_l1_data_gas_mismatch", "The number of times the L1 data gas in a proposal does not match the value expected by this validator", init = 0 },
        MetricGauge { CONSENSUS_L2_GAS_PRICE, "consensus_l2_gas_price", "The L2 gas price calculated in an accepted proposal" },
        MetricCounter { CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR, "consensus_l1_gas_price_provider_error", "Number of times the context got an error when querying the L1 gas price provider", init=0},

        // Cende metrics
        MetricGauge { CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER, "cende_last_prepared_blob_block_number", "The blob block number that cende knows. That means the sequencer can be the proposer only if the current height is greater by one than this value." },
        MetricHistogram { CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY, "cende_prepare_blob_for_next_height_latency", "The time it takes to prepare the blob for the next height, i.e create the blob object." },
        // TODO(dvir): consider to differ the case when the blob was already written, that will prevent using the `sequencer_latency_histogram` attribute.
        // TODO(dvir): add a counter for successful blob writes and failed blob writes.
        MetricHistogram { CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY, "cende_write_prev_height_blob_latency", "Be careful with this metric, if the blob was already written by another request, the latency is much lower since writing to Aerospike is not needed." },
        MetricCounter { CENDE_WRITE_BLOB_SUCCESS , "cende_write_blob_success", "The number of successful blob writes to Aerospike", init = 0 },
        LabeledMetricCounter { CENDE_WRITE_BLOB_FAILURE , "cende_write_blob_failure", "The number of failed blob writes to Aerospike", init = 0, labels = CENDE_WRITE_BLOB_FAILURE_REASON },
    }
);

pub const LABEL_CENDE_FAILURE_REASON: &str = "cende_write_failure_reason";

#[derive(IntoStaticStr, EnumIter, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum CendeWriteFailureReason {
    SkipWriteHeight,
    CommunicationError,
    CendeRecorderError,
    BlobNotAvailable,
    HeightMismatch,
}

generate_permutation_labels! {
    CENDE_WRITE_BLOB_FAILURE_REASON,
    (LABEL_CENDE_FAILURE_REASON, CendeWriteFailureReason),
}

pub(crate) fn record_write_failure(reason: CendeWriteFailureReason) {
    CENDE_WRITE_BLOB_FAILURE.increment(1, &[(LABEL_CENDE_FAILURE_REASON, reason.into())]);
}

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.register();
    CONSENSUS_NUM_TXS_IN_PROPOSAL.register();
    CONSENSUS_L1_GAS_MISMATCH.register();
    CONSENSUS_L1_DATA_GAS_MISMATCH.register();
    CONSENSUS_L2_GAS_PRICE.register();
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.register();
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.register();
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.register();
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.register();
    CENDE_WRITE_BLOB_SUCCESS.register();
    CENDE_WRITE_BLOB_FAILURE.register();
}
