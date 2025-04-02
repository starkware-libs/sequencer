use apollo_sequencer_metrics::define_metrics;
use apollo_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in a valid proposal received" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in a valid proposal received" },
        MetricCounter { CONSENSUS_CONVERSION_RATE_MISMATCH_COUNT, "consensus_conversion_rate_mismatch", "The number of proposals that failed because of eth to fri conversion rate mismatch", init = 0 },
    }
);

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.register();
    CONSENSUS_NUM_TXS_IN_PROPOSAL.register();
    CONSENSUS_CONVERSION_RATE_MISMATCH_COUNT.register();
}
