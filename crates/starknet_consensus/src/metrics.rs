use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_BLOCK_NUMBER, "consensus_block_number", "The block number consensus is working to decide" },
        MetricGauge { CONSENSUS_ROUND, "consensus_round", "The round of the state machine"},
        MetricGauge { CONSENSUS_MAX_CACHED_HEIGHT, "consesnus_max_cached_height", "How many heights above current are cached"},
        MetricGauge { CONSENSUS_CACHED_MESSAGES, "consensus_cached_messages", "The number of messages cached when starting a new height" },
        MetricCounter { CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS, "consensus_decisions_reached_by_consensus", "The total number of decisions reached by way of consensus", init=0},
        MetricCounter { CONSENSUS_DECISIONS_REACHED_BY_SYNC, "consensus_decisions_reached_by_sync", "The total number of decisions reached by way of sync", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_RECEIVED, "consensus_proposals_received", "The total number of proposals received by the consensus", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_VALID_INIT, "consensus_proposals_valid_init", "The total number of proposals received with a valid init by the consensus", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_VALIDATED, "consensus_proposals_validated", "The total number of proposals successfully validated by the consensus", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_FAILED, "consensus_proposals_failed", "The total number of proposals failed to be validated by the consensus", init=0},
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_BLOCK_NUMBER.register();
    CONSENSUS_ROUND.register();
    CONSENSUS_MAX_CACHED_HEIGHT.register();
    CONSENSUS_CACHED_MESSAGES.register();
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.register();
    CONSENSUS_DECISIONS_REACHED_BY_SYNC.register();
    CONSENSUS_PROPOSALS_RECEIVED.register();
    CONSENSUS_PROPOSALS_VALID_INIT.register();
    CONSENSUS_PROPOSALS_VALIDATED.register();
    CONSENSUS_PROPOSALS_FAILED.register();
}
