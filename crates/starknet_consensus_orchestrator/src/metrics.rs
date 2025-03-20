use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::MetricGauge;

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_NUM_BATCHES_IN_PROPOSAL, "consensus_num_batches_in_proposal", "The number of transaction batches in the current proposal" },
        MetricGauge { CONSENSUS_NUM_TXS_IN_PROPOSAL, "consensus_num_txs_in_proposal", "The total number of individual transactions in the current proposal" },
    }
);
