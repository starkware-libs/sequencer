use starknet_sequencer_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge};
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};
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
        LabeledMetricCounter { CONSENSUS_TIMEOUTS, "consensus_timeouts", "The number of timeouts for the current block number", init=0, labels = CONSENSUS_TIMEOUT_LABELS },
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
    CONSENSUS_REPROPOSALS.register();
}
