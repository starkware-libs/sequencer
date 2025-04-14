use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricGauge;

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in a valid proposal received" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in a valid proposal received" },
        MetricGauge { CONSENSUS_L2_GAS_PRICE, "consensus_l2_gas_price", "The L2 gas price calculated in a valid proposal received" },
    }
);

pub(crate) fn register_metrics() {
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.register();
    CONSENSUS_NUM_TXS_IN_PROPOSAL.register();
    CONSENSUS_L2_GAS_PRICE.register();
}
