use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{EnumIter, IntoStaticStr};

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
        MetricCounter { CONSENSUS_BUILD_PROPOSAL_TOTAL, "consensus_build_proposal_total", "The total number of proposals built", init=0},
        MetricCounter { CONSENSUS_BUILD_PROPOSAL_FAILED, "consensus_build_proposal_failed", "The number of proposals that failed to be built", init=0},
        MetricCounter { CONSENSUS_REPROPOSALS, "consensus_reproposals", "The number of reproposals sent", init=0},
        MetricCounter { CONSENSUS_NEW_VALUE_LOCKS, "consensus_new_value_locks", "The number of times consensus has attained a lock on a new value", init=0},
        MetricCounter { CONSENSUS_HELD_LOCKS, "consensus_held_locks", "The number of times consensus progressed to a new round while holding a lock", init=0},
        MetricCounter { CONSENSUS_OUTBOUND_STREAM_STARTED, "consensus_outbound_stream_started", "The total number of outbound streams started", init=0 },
        MetricCounter { CONSENSUS_OUTBOUND_STREAM_FINISHED, "consensus_outbound_stream_finished", "The total number of outbound streams finished", init=0 },
        MetricCounter { CONSENSUS_INBOUND_STREAM_STARTED, "consensus_inbound_stream_started", "The total number of inbound streams started", init=0 },
        MetricCounter { CONSENSUS_INBOUND_STREAM_EVICTED, "consensus_inbound_stream_evicted", "The total number of inbound streams evicted due to cache capacity", init=0 },
        MetricCounter { CONSENSUS_INBOUND_STREAM_FINISHED, "consensus_inbound_stream_finished", "The total number of inbound streams finished", init=0 },
        // TODO(Matan): remove this metric.
        MetricCounter { CONSENSUS_ROUND_ABOVE_ZERO, "consensus_round_above_zero", "The number of times the consensus round has increased above zero", init=0 },
        MetricCounter { CONSENSUS_CONFLICTING_VOTES, "consensus_conflicting_votes", "The number of times consensus has received conflicting votes", init=0 },
        LabeledMetricCounter { CONSENSUS_TIMEOUTS, "consensus_timeouts", "The number of times consensus has timed out", init=0, labels = CONSENSUS_TIMEOUT_LABELS },
    },
);

pub const LABEL_NAME_TIMEOUT_REASON: &str = "timeout_reason";

#[derive(IntoStaticStr, EnumIter, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum TimeoutReason {
    Propose,
    Prevote,
    Precommit,
}

generate_permutation_labels! {
    CONSENSUS_TIMEOUT_LABELS,
    (LABEL_NAME_TIMEOUT_REASON, TimeoutReason),
}

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
    CONSENSUS_BUILD_PROPOSAL_TOTAL.register();
    CONSENSUS_BUILD_PROPOSAL_FAILED.register();
    CONSENSUS_NEW_VALUE_LOCKS.register();
    CONSENSUS_HELD_LOCKS.register();
    CONSENSUS_REPROPOSALS.register();
    CONSENSUS_INBOUND_STREAM_STARTED.register();
    CONSENSUS_INBOUND_STREAM_EVICTED.register();
    CONSENSUS_INBOUND_STREAM_FINISHED.register();
    CONSENSUS_OUTBOUND_STREAM_STARTED.register();
    CONSENSUS_OUTBOUND_STREAM_FINISHED.register();
    CONSENSUS_ROUND_ABOVE_ZERO.register();
    CONSENSUS_CONFLICTING_VOTES.register();
    CONSENSUS_TIMEOUTS.register();
}
