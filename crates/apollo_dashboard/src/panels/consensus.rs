use apollo_batcher::metrics::PRECONFIRMED_BLOCK_WRITTEN;
use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_BUILD_PROPOSAL_TOTAL,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_AS_PROPOSER,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_PROPOSALS_RECEIVED,
    CONSENSUS_PROPOSALS_VALIDATED,
    CONSENSUS_ROUND,
    CONSENSUS_ROUND_ABOVE_ZERO,
    CONSENSUS_ROUND_ADVANCES,
    CONSENSUS_TIMEOUTS,
    LABEL_NAME_TIMEOUT_TYPE,
};
use apollo_consensus_manager::metrics::{
    CONSENSUS_NETWORK_EVENTS,
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    CONSENSUS_VOTES_NUM_DROPPED_MESSAGES,
    CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use apollo_consensus_orchestrator::metrics::{
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_BUILD_PROPOSAL_FAILURE,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_VALIDATE_PROPOSAL_FAILURE,
    LABEL_BUILD_PROPOSAL_FAILURE_REASON,
    LABEL_CENDE_FAILURE_REASON,
    LABEL_VALIDATE_PROPOSAL_FAILURE_REASON,
};
use apollo_metrics::MetricCommon;
use apollo_network::network_manager::metrics::{
    LABEL_NAME_BROADCAST_DROP_REASON,
    LABEL_NAME_EVENT_TYPE,
};
use apollo_state_sync_metrics::metrics::STATE_SYNC_CLASS_MANAGER_MARKER;

use crate::dashboard::{Panel, PanelType, Row, Unit};
use crate::query_builder;
use crate::query_builder::{DEFAULT_DURATION, RANGE_DURATION};

// The key events that are relevant to the consensus panel.
const CONSENSUS_KEY_EVENTS_LOG_QUERY: &str =
    "\"START_HEIGHT:\" OR \"START_ROUND\" OR textPayload=~\"DECISION_REACHED\" OR \
     \"PROPOSAL_FAILED\" OR \"Proposal succeeded\" OR \"Applying Timeout\" OR \"Accepting\" OR \
     \"Broadcasting\"";

fn get_panel_consensus_block_number() -> Panel {
    Panel::new(
        "Consensus Height",
        "The block height the node is currently working on",
        CONSENSUS_BLOCK_NUMBER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query(
        "\"START_HEIGHT: running consensus for height\" OR \"Start building proposal\" OR \"Start \
         validating proposal\"",
    )
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

pub(crate) fn get_panel_consensus_block_number_diff_from_sync() -> Panel {
    Panel::new(
        "Consensus Height Diff From Sync",
        "The difference between the consensus height and the sync height",
        format!(
            "({} - {})",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_panel_consensus_round() -> Panel {
    Panel::new(
        "Consensus Round",
        "The round the node is currently working on",
        CONSENSUS_ROUND.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_log_query("\"START_ROUND\" OR \"PROPOSAL_FAILED\" OR textPayload=~\"DECISION_REACHED\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

pub(crate) fn get_panel_consensus_round_advanced() -> Panel {
    Panel::new(
        "Consensus Round Advanced",
        format!(
            "The number of times the consensus round advanced (counter is increased whenever \
             round > 0) ({DEFAULT_DURATION} window)",
        ),
        query_builder::increase(&CONSENSUS_ROUND_ADVANCES, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("\"START_ROUND\" OR \"PROPOSAL_FAILED\" OR textPayload=~\"DECISION_REACHED\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_round_above_zero() -> Panel {
    Panel::new(
        "Consensus Round Above Zero",
        "Occurances where the consensus round was 1, relative to displayed range",
        format!(
            "{m} - ({m} @ start())",
            m = CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter().to_string()
        ),
        PanelType::TimeSeries,
    )
    .with_log_query("\"START_ROUND\" OR \"PROPOSAL_FAILED\" OR textPayload=~\"DECISION_REACHED\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

pub(crate) fn get_panel_consensus_block_time_avg() -> Panel {
    Panel::new(
        "Average Block Time",
        "Average block time (1m window)",
        format!("1 / rate({}[1m])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_consensus_decisions_reached_by_consensus() -> Panel {
    Panel::new(
        "Decisions Reached By Consensus",
        format!("The number of decisions reached by way of consensus ({DEFAULT_DURATION} window)",),
        query_builder::increase(&CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("DECISION_REACHED: Decision reached for round")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_decisions_reached_by_sync() -> Panel {
    Panel::new(
        "Decisions Reached By Sync",
        format!("The number of decisions reached by way of sync ({DEFAULT_DURATION} window)",),
        query_builder::increase(&CONSENSUS_DECISIONS_REACHED_BY_SYNC, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("Decision learned via sync protocol.")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_proposals_received() -> Panel {
    Panel::new(
        "Proposal Validation: Number of Received Proposals",
        format!("The number of proposals received from the network ({DEFAULT_DURATION} window)",),
        query_builder::increase(&CONSENSUS_PROPOSALS_RECEIVED, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}

fn get_panel_consensus_proposals_validated() -> Panel {
    Panel::new(
        "Proposal Validation: Number of Validated Proposals",
        format!(
            "The number of proposals received and validated successfully ({DEFAULT_DURATION} \
             window)",
        ),
        query_builder::increase(&CONSENSUS_PROPOSALS_VALIDATED, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("\"Validated proposal.\" OR \"PROPOSAL_FAILED\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_proposals_invalid() -> Panel {
    Panel::new(
        "Proposal Validation: Number of Invalid Proposals",
        format!(
            "The number of proposals received and failed validation ({DEFAULT_DURATION} window)",
        ),
        query_builder::increase(&CONSENSUS_PROPOSALS_INVALID, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("\"Validated proposal.\" OR \"PROPOSAL_FAILED\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_validate_proposal_failure() -> Panel {
    Panel::new(
        "Proposal Validation: Proposal Failure by Reason",
        "The number of validate proposal failures (over the selected time range)",
        query_builder::sum_by_label(
            &CONSENSUS_VALIDATE_PROPOSAL_FAILURE,
            LABEL_VALIDATE_PROPOSAL_FAILURE_REASON,
            query_builder::DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
    .with_log_query("PROPOSAL_FAILED: Proposal failed as validator")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_build_proposal_total() -> Panel {
    Panel::new(
        "Proposal Build: Number of Proposals Started",
        format!("The number of proposals that started building ({DEFAULT_DURATION} window)",),
        query_builder::increase(&CONSENSUS_BUILD_PROPOSAL_TOTAL, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}

fn get_panel_consensus_build_proposal_failed() -> Panel {
    Panel::new(
        "Proposal Build: Number of Proposals Failed",
        format!("The number of proposals that failed to be built ({DEFAULT_DURATION} window)",),
        query_builder::increase(&CONSENSUS_BUILD_PROPOSAL_FAILED, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}

fn get_panel_build_proposal_failure() -> Panel {
    Panel::new(
        "Proposal Build: Proposal Failure by Reason",
        "The number of build proposal failures (over the selected time range)",
        query_builder::sum_by_label(
            &CONSENSUS_BUILD_PROPOSAL_FAILURE,
            LABEL_BUILD_PROPOSAL_FAILURE_REASON,
            query_builder::DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
    .with_log_query("PROPOSAL_FAILED: Proposal failed as proposer")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_timeouts_by_type() -> Panel {
    Panel::new(
        "Consensus Timeouts By Type",
        format!(
            "The number of times consensus has timed out by type ({} window). \n- TimeoutPropose: \
             as proposer, didn’t finish building in time; as validator, either didn’t receive the \
             proposal or didn’t finish validation in time.\n- TimeoutPrevote: the node voted and \
             received a quorum of prevotes, but not on the same value.\n- TimeoutPrecommit: \
             didn’t finish validation but quorum of precommits received, or it finished but no \
             decision reached.",
            DEFAULT_DURATION
        ),
        query_builder::sum_by_label(
            &CONSENSUS_TIMEOUTS,
            LABEL_NAME_TIMEOUT_TYPE,
            query_builder::DisplayMethod::Increase(DEFAULT_DURATION),
            false,
        ),
        PanelType::TimeSeries,
    )
    .with_log_query("Applying Timeout")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

fn get_panel_consensus_l2_gas_price() -> Panel {
    Panel::new(
        "L2 Gas Price (GFri)",
        "L2 gas price in GFri calculated in an accepted proposal",
        format!("{} / 1e9", CONSENSUS_L2_GAS_PRICE.get_name_with_filter()),
        PanelType::TimeSeries,
    )
}

fn get_panel_cende_last_prepared_blob_block_number() -> Panel {
    Panel::new(
        "Last Prepared Blob Block Number",
        "The block number that is ready to be sent to Cende in the next height",
        CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query("Blob for block number")
}

fn get_panel_cende_write_prev_height_blob_latency() -> Panel {
    Panel::from_hist(
        &CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
        "Write Blob Latency",
        "The time it takes to write the blob to Cende",
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_cende_write_blob_success() -> Panel {
    let query_expression = [
        "\"Blob for block number\"",
        "\"Writing blob to Aerospike\"",
        "\"transactions was written to Aerospike\"",
    ]
    .join(" OR ");

    Panel::new(
        "Write Blob Success",
        format!("The number of successful blob writes to Cende ({DEFAULT_DURATION} window)"),
        query_builder::increase(&CENDE_WRITE_BLOB_SUCCESS, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query(query_expression)
}

fn get_panel_cende_write_blob_failure() -> Panel {
    Panel::new(
        "Write Blob Failure by Reason",
        format!("The number of failed blob writes to Cende ({} window)", DEFAULT_DURATION),
        query_builder::sum_by_label(
            &CENDE_WRITE_BLOB_FAILURE,
            LABEL_CENDE_FAILURE_REASON,
            query_builder::DisplayMethod::Increase(DEFAULT_DURATION),
            false,
        ),
        PanelType::TimeSeries,
    )
    .with_log_query("CENDE_FAILURE")
}

fn get_panel_cende_write_preconfirmed_block() -> Panel {
    Panel::new(
        "Write Preconfirmed Block Success",
        format!(
            "The number of successful writes to Cende for preconfirmed blocks ({DEFAULT_DURATION} \
             window). Each preconfirmed block may involve multiple writes.",
        ),
        query_builder::increase(&PRECONFIRMED_BLOCK_WRITTEN, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("write_pre_confirmed_block request succeeded.")
}

fn get_panel_consensus_num_connected_peers() -> Panel {
    Panel::new(
        "Number of Connected Peers",
        "The number of connected peers in Consensus P2P",
        CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}

fn get_panel_consensus_votes_num_sent_messages() -> Panel {
    Panel::new(
        "Consensus Votes Number of Sent Messages",
        "The increase in the number of vote messages sent by consensus p2p (over the selected \
         time range)",
        query_builder::increase(&CONSENSUS_VOTES_NUM_SENT_MESSAGES, "$__range"),
        PanelType::Stat,
    )
}

fn get_panel_consensus_votes_num_received_messages() -> Panel {
    Panel::new(
        "Consensus Votes Number of Received Messages",
        "The increase in the number of vote messages received by consensus p2p (over the selected \
         time range)",
        query_builder::increase(&CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, "$__range"),
        PanelType::Stat,
    )
}

fn get_panel_consensus_proposals_num_sent_messages() -> Panel {
    Panel::new(
        "Consensus Proposals Number of Sent Messages",
        "The increase in the number of proposal messages sent by consensus p2p (over the selected \
         time range)",
        query_builder::increase(&CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, "$__range"),
        PanelType::Stat,
    )
}

fn get_panel_consensus_proposals_num_received_messages() -> Panel {
    Panel::new(
        "Consensus Proposals Number of Received Messages",
        "The increase in the number of proposal messages received by consensus p2p (over the \
         selected time range)",
        query_builder::increase(&CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, "$__range"),
        PanelType::Stat,
    )
}

fn get_panel_consensus_conflicting_votes() -> Panel {
    Panel::new(
        "Consensus Conflicting Votes",
        "The increase in the number of conflicting votes (over the selected time range)",
        query_builder::increase(&CONSENSUS_CONFLICTING_VOTES, "$__range"),
        PanelType::Stat,
    )
}

fn get_panel_consensus_network_events_by_type() -> Panel {
    Panel::new(
        "Consensus Network Events By Type",
        "Network events received by consensus p2p, by event type (over the selected time range)",
        query_builder::sum_by_label(
            &CONSENSUS_NETWORK_EVENTS,
            LABEL_NAME_EVENT_TYPE,
            query_builder::DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
}

fn get_panel_consensus_votes_dropped_messages_by_reason() -> Panel {
    Panel::new(
        "Consensus Votes Dropped Messages By Reason",
        "The number of dropped consensus votes messages, by reason (over the selected time range)",
        query_builder::sum_by_label(
            &CONSENSUS_VOTES_NUM_DROPPED_MESSAGES,
            LABEL_NAME_BROADCAST_DROP_REASON,
            query_builder::DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
}

fn get_panel_consensus_proposals_dropped_messages_by_reason() -> Panel {
    Panel::new(
        "Consensus Proposals Dropped Messages By Reason",
        "The number of dropped consensus proposals messages, by reason (over the selected time \
         range)",
        query_builder::sum_by_label(
            &CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES,
            LABEL_NAME_BROADCAST_DROP_REASON,
            query_builder::DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
}

fn get_panel_consensus_decisions_reached_as_proposer() -> Panel {
    Panel::new(
        "Consensus Decisions Reached As Proposer",
        format!(
            "The number of rounds with decision reached where this node is the proposer \
             ({DEFAULT_DURATION} window)",
        ),
        query_builder::increase(&CONSENSUS_DECISIONS_REACHED_AS_PROPOSER, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("\"Building proposal\" OR \"BATCHER_FIN_PROPOSER\"")
    .with_log_comment(CONSENSUS_KEY_EVENTS_LOG_QUERY)
}

pub(crate) fn get_consensus_row() -> Row {
    Row::new(
        "Consensus",
        vec![
            get_panel_consensus_block_number(),
            get_panel_consensus_round(),
            get_panel_consensus_round_advanced(),
            get_panel_consensus_block_time_avg(),
            get_panel_consensus_round_above_zero(),
            get_panel_consensus_block_number_diff_from_sync(),
            get_panel_consensus_decisions_reached_as_proposer(),
            get_panel_consensus_decisions_reached_by_consensus(),
            get_panel_consensus_decisions_reached_by_sync(),
            get_panel_consensus_build_proposal_total(),
            get_panel_consensus_build_proposal_failed(),
            get_panel_build_proposal_failure(),
            get_panel_consensus_proposals_received(),
            get_panel_consensus_proposals_validated(),
            get_panel_consensus_proposals_invalid(),
            get_panel_validate_proposal_failure(),
            get_panel_consensus_timeouts_by_type(),
            get_panel_consensus_l2_gas_price(),
        ],
    )
}

pub(crate) fn get_cende_row() -> Row {
    Row::new(
        "Cende",
        vec![
            get_panel_cende_write_blob_success(),
            get_panel_cende_write_blob_failure(),
            get_panel_cende_write_prev_height_blob_latency(),
            get_panel_cende_last_prepared_blob_block_number(),
            get_panel_cende_write_preconfirmed_block(),
        ],
    )
}

pub(crate) fn get_consensus_p2p_row() -> Row {
    Row::new(
        "ConsensusP2p",
        vec![
            get_panel_consensus_num_connected_peers(),
            get_panel_consensus_votes_num_sent_messages(),
            get_panel_consensus_votes_num_received_messages(),
            get_panel_consensus_votes_dropped_messages_by_reason(),
            get_panel_consensus_conflicting_votes(),
            get_panel_consensus_proposals_num_sent_messages(),
            get_panel_consensus_proposals_num_received_messages(),
            get_panel_consensus_proposals_dropped_messages_by_reason(),
            get_panel_consensus_network_events_by_type(),
        ],
    )
}
