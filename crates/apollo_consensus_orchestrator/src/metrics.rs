use apollo_metrics::define_metrics;
use apollo_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in a valid proposal received" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in a valid proposal received" },
        MetricCounter { CONSENSUS_L1_GAS_MISMATCH, "consensus_l1_gas_mismatch", "The number of times the L1 gas in a proposal does not match the L1 gas as queried by the validator", init = 0 },
        MetricCounter { CONSENSUS_L1_DATA_GAS_MISMATCH, "consensus_l1_data_gas_mismatch", "The number of times the L1 data gas in a proposal does not match the L1 data gas as queried by the validator", init = 0 },
    }
);

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.register();
    CONSENSUS_NUM_TXS_IN_PROPOSAL.register();
    CONSENSUS_L1_GAS_MISMATCH.register();
    CONSENSUS_L1_DATA_GAS_MISMATCH.register();
}
