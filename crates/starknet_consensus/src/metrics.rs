use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::MetricGauge;

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_BLOCK_NUMBER, "consensus_block_number", "The block number consensus is working to decide" },
        MetricGauge { CONSENSUS_ROUND, "consensus_round", "The round of the state machine"},
        MetricGauge { CONSENSUS_MAX_CACHED_HEIGHT, "consesnus_max_cached_height", "How many heights above current are cached"},
        MetricGauge { CONSENSUS_CACHED_MESSAGES, "consensus_cached_messages", "How many messages are cached when starting to work on a new block number" },
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_BLOCK_NUMBER.register();
    CONSENSUS_ROUND.register();
    CONSENSUS_MAX_CACHED_HEIGHT.register();
    CONSENSUS_CACHED_MESSAGES.register();
}
