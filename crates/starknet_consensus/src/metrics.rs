use starknet_sequencer_metrics::define_metrics;
use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

define_metrics!(
    Consensus => {
        MetricGauge { CONSENSUS_BLOCK_NUMBER, "consensus_block_number", "The block number consensus is working to decide" },
        MetricGauge { CONSENSUS_ROUND, "consensus_round", "The round of the state machine"},
        MetricGauge { CONSENSUS_MAX_CACHED_BLOCK_NUMBER, "consensus_max_cached_block_number", "How many blocks after current are cached"},
        MetricGauge { CONSENSUS_CACHED_VOTES, "consensus_cached_votes", "How many votes are cached when starting to work on a new block number" },
        MetricCounter { CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS, "consensus_decisions_reached_by_consensus", "The total number of decisions reached by way of consensus", init=0},
        MetricCounter { CONSENSUS_DECISIONS_REACHED_BY_SYNC, "consensus_decisions_reached_by_sync", "The total number of decisions reached by way of sync", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_RECEIVED, "consensus_proposals_received", "The total number of proposals received", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_VALID_INIT, "consensus_proposals_valid_init", "The total number of proposals received with a valid init", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_VALIDATED, "consensus_proposals_validated", "The total number of complete, valid proposals received", init=0},
        MetricCounter { CONSENSUS_PROPOSALS_INVALID, "consensus_proposals_invalid", "The total number of proposals that failed validation", init=0},
    },
);

pub(crate) fn register_metrics() {
    CONSENSUS_BLOCK_NUMBER.register();
    CONSENSUS_ROUND.register();
    CONSENSUS_MAX_CACHED_BLOCK_NUMBER.register();
    CONSENSUS_CACHED_VOTES.register();
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.register();
    CONSENSUS_DECISIONS_REACHED_BY_SYNC.register();
    CONSENSUS_PROPOSALS_RECEIVED.register();
    CONSENSUS_PROPOSALS_VALID_INIT.register();
    CONSENSUS_PROPOSALS_VALIDATED.register();
    CONSENSUS_PROPOSALS_INVALID.register();
}
