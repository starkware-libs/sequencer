use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::MetricGauge;

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_BLOCK_NUMBER, "consensus_block_number", "The block number consensus is working to decide" },
        MetricGauge { CONSENSUS_ROUND, "consensus_round", "The round of the state machine"}
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_BLOCK_NUMBER.register();
    CONSENSUS_ROUND.register();
}
