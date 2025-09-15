use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_BUILD_PROPOSAL_TOTAL,
    CONSENSUS_CACHED_VOTES,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_HELD_LOCKS,
    CONSENSUS_INBOUND_STREAM_EVICTED,
    CONSENSUS_INBOUND_STREAM_FINISHED,
    CONSENSUS_INBOUND_STREAM_STARTED,
    CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    CONSENSUS_NEW_VALUE_LOCKS,
    CONSENSUS_OUTBOUND_STREAM_FINISHED,
    CONSENSUS_OUTBOUND_STREAM_STARTED,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_PROPOSALS_RECEIVED,
    CONSENSUS_PROPOSALS_VALIDATED,
    CONSENSUS_PROPOSALS_VALID_INIT,
    CONSENSUS_REPROPOSALS,
    CONSENSUS_ROUND,
    CONSENSUS_ROUND_ABOVE_ZERO,
    CONSENSUS_TIMEOUTS,
    LABEL_NAME_TIMEOUT_REASON,
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
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_ETH_TO_FRI_RATE_MISMATCH,
    CONSENSUS_L1_DATA_GAS_MISMATCH,
    CONSENSUS_L1_GAS_MISMATCH,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
    LABEL_CENDE_FAILURE_REASON,
};
use apollo_network::network_manager::metrics::{
    LABEL_NAME_BROADCAST_DROP_REASON,
    LABEL_NAME_EVENT_TYPE,
};
use apollo_state_sync_metrics::metrics::STATE_SYNC_CLASS_MANAGER_MARKER;

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_consensus_block_number() -> Panel {
    Panel::from(&CONSENSUS_BLOCK_NUMBER)
}
fn get_panel_consensus_block_number_diff_between_nodes() -> Panel {
    Panel::new(
        "block_number_diff_between_nodes",
        "Block number diff between nodes",
        vec![format!(
            "(max({}) - min({}))",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_block_number_diff_from_sync() -> Panel {
    Panel::new(
        "consensus_sync_block_number_diff",
        "The difference between the consensus block number and the sync block number",
        vec![format!(
            "({} - {})",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_round() -> Panel {
    Panel::from(&CONSENSUS_ROUND)
}
fn get_panel_consensus_round_avg() -> Panel {
    Panel::new(
        "Average consensus round",
        "Average consensus round (10m)",
        vec![format!("avg_over_time({}[10m])", CONSENSUS_ROUND.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_round_above_zero() -> Panel {
    Panel::from(&CONSENSUS_ROUND_ABOVE_ZERO)
}
fn get_panel_consensus_max_cached_block_number() -> Panel {
    Panel::from(&CONSENSUS_MAX_CACHED_BLOCK_NUMBER)
}
fn get_panel_consensus_cached_votes() -> Panel {
    Panel::from(&CONSENSUS_CACHED_VOTES)
}
fn get_panel_consensus_decisions_reached_by_consensus() -> Panel {
    Panel::from(&CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS)
}
fn get_panel_consensus_decisions_reached_by_sync() -> Panel {
    Panel::from(&CONSENSUS_DECISIONS_REACHED_BY_SYNC)
}
fn get_panel_consensus_inbound_stream_started() -> Panel {
    Panel::from(&CONSENSUS_INBOUND_STREAM_STARTED)
}
fn get_panel_consensus_inbound_stream_evicted() -> Panel {
    Panel::from(&CONSENSUS_INBOUND_STREAM_EVICTED)
}
fn get_panel_consensus_inbound_stream_finished() -> Panel {
    Panel::from(&CONSENSUS_INBOUND_STREAM_FINISHED)
}
fn get_panel_consensus_outbound_stream_started() -> Panel {
    Panel::from(&CONSENSUS_OUTBOUND_STREAM_STARTED)
}
fn get_panel_consensus_outbound_stream_finished() -> Panel {
    Panel::from(&CONSENSUS_OUTBOUND_STREAM_FINISHED)
}
fn get_panel_consensus_proposals_received() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_RECEIVED)
}
fn get_panel_consensus_proposals_valid_init() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_VALID_INIT)
}
fn get_panel_consensus_proposals_validated() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_VALIDATED)
}
fn get_panel_consensus_proposals_invalid() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_INVALID)
}
fn get_panel_consensus_build_proposal_total() -> Panel {
    Panel::from(&CONSENSUS_BUILD_PROPOSAL_TOTAL)
}
fn get_panel_consensus_build_proposal_failed() -> Panel {
    Panel::from(&CONSENSUS_BUILD_PROPOSAL_FAILED)
}
fn get_panel_consensus_reproposals() -> Panel {
    Panel::from(&CONSENSUS_REPROPOSALS)
}
fn get_panel_consensus_new_value_locks() -> Panel {
    Panel::from(&CONSENSUS_NEW_VALUE_LOCKS)
}
fn get_panel_consensus_held_locks() -> Panel {
    Panel::from(&CONSENSUS_HELD_LOCKS)
}
fn get_panel_consensus_timeouts_by_type() -> Panel {
    Panel::new(
        CONSENSUS_TIMEOUTS.get_name(),
        CONSENSUS_TIMEOUTS.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            LABEL_NAME_TIMEOUT_REASON,
            CONSENSUS_TIMEOUTS.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_num_batches_in_proposal() -> Panel {
    Panel::from(&CONSENSUS_NUM_BATCHES_IN_PROPOSAL)
}
fn get_panel_consensus_num_txs_in_proposal() -> Panel {
    Panel::from(&CONSENSUS_NUM_TXS_IN_PROPOSAL)
}
fn get_panel_consensus_l2_gas_price() -> Panel {
    Panel::from(&CONSENSUS_L2_GAS_PRICE)
}
fn get_panel_consensus_num_connected_peers() -> Panel {
    Panel::from(&CONSENSUS_NUM_CONNECTED_PEERS)
}
fn get_panel_consensus_votes_num_sent_messages() -> Panel {
    Panel::from(&CONSENSUS_VOTES_NUM_SENT_MESSAGES)
}
fn get_panel_consensus_votes_num_received_messages() -> Panel {
    Panel::from(&CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES)
}
fn get_panel_consensus_proposals_num_sent_messages() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES)
}
fn get_panel_consensus_proposals_num_received_messages() -> Panel {
    Panel::from(&CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES)
}
fn get_panel_consensus_conflicting_votes() -> Panel {
    Panel::from(&CONSENSUS_CONFLICTING_VOTES)
}
fn get_panel_cende_last_prepared_blob_block_number() -> Panel {
    Panel::from(&CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER)
}
fn get_panel_cende_prepare_blob_for_next_height_latency() -> Panel {
    Panel::from(&CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY)
}
fn get_panel_cende_write_prev_height_blob_latency() -> Panel {
    Panel::from(&CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY)
}
fn get_panel_cende_write_blob_success() -> Panel {
    Panel::from(&CENDE_WRITE_BLOB_SUCCESS)
}
fn get_panel_cende_write_blob_failure() -> Panel {
    Panel::new(
        CENDE_WRITE_BLOB_FAILURE.get_name(),
        CENDE_WRITE_BLOB_FAILURE.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            LABEL_CENDE_FAILURE_REASON,
            CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_eth_to_fri_rate_mismatch() -> Panel {
    Panel::from(&CONSENSUS_ETH_TO_FRI_RATE_MISMATCH)
}
fn get_panel_consensus_l1_data_gas_mismatch() -> Panel {
    Panel::from(&CONSENSUS_L1_DATA_GAS_MISMATCH)
}
fn get_panel_consensus_l1_gas_mismatch() -> Panel {
    Panel::from(&CONSENSUS_L1_GAS_MISMATCH)
}

fn get_panel_consensus_network_events_by_type() -> Panel {
    Panel::new(
        CONSENSUS_NETWORK_EVENTS.get_name(),
        CONSENSUS_NETWORK_EVENTS.get_description(),
        vec![format!(
            "sum by ({}) ({})",
            LABEL_NAME_EVENT_TYPE,
            CONSENSUS_NETWORK_EVENTS.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_consensus_votes_dropped_messages_by_reason() -> Panel {
    Panel::new(
        CONSENSUS_VOTES_NUM_DROPPED_MESSAGES.get_name(),
        CONSENSUS_VOTES_NUM_DROPPED_MESSAGES.get_description(),
        vec![format!(
            "sum by ({}) ({})",
            LABEL_NAME_BROADCAST_DROP_REASON,
            CONSENSUS_VOTES_NUM_DROPPED_MESSAGES.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_consensus_proposals_dropped_messages_by_reason() -> Panel {
    Panel::new(
        CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES.get_name(),
        CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES.get_description(),
        vec![format!(
            "sum by ({}) ({})",
            LABEL_NAME_BROADCAST_DROP_REASON,
            CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_consensus_row() -> Row {
    Row::new(
        "Consensus",
        vec![
            get_panel_consensus_block_number(),
            get_panel_consensus_round(),
            get_panel_consensus_round_avg(),
            get_panel_consensus_round_above_zero(),
            get_panel_consensus_block_number_diff_between_nodes(),
            get_panel_consensus_block_number_diff_from_sync(),
            get_panel_consensus_max_cached_block_number(),
            get_panel_consensus_cached_votes(),
            get_panel_consensus_decisions_reached_by_consensus(),
            get_panel_consensus_decisions_reached_by_sync(),
            get_panel_consensus_proposals_received(),
            get_panel_consensus_proposals_valid_init(),
            get_panel_consensus_proposals_validated(),
            get_panel_consensus_proposals_invalid(),
            get_panel_consensus_build_proposal_total(),
            get_panel_consensus_build_proposal_failed(),
            get_panel_consensus_reproposals(),
            get_panel_consensus_new_value_locks(),
            get_panel_consensus_held_locks(),
            get_panel_consensus_timeouts_by_type(),
            get_panel_consensus_num_batches_in_proposal(),
            get_panel_consensus_num_txs_in_proposal(),
            get_panel_consensus_inbound_stream_started(),
            get_panel_consensus_inbound_stream_evicted(),
            get_panel_consensus_inbound_stream_finished(),
            get_panel_consensus_outbound_stream_started(),
            get_panel_consensus_outbound_stream_finished(),
            get_panel_consensus_l2_gas_price(),
            get_panel_cende_last_prepared_blob_block_number(),
            get_panel_cende_prepare_blob_for_next_height_latency(),
            get_panel_cende_write_prev_height_blob_latency(),
            get_panel_cende_write_blob_success(),
            get_panel_cende_write_blob_failure(),
            get_panel_consensus_eth_to_fri_rate_mismatch(),
            get_panel_consensus_l1_data_gas_mismatch(),
            get_panel_consensus_l1_gas_mismatch(),
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
            get_panel_consensus_proposals_num_sent_messages(),
            get_panel_consensus_proposals_num_received_messages(),
            get_panel_consensus_conflicting_votes(),
            get_panel_consensus_network_events_by_type(),
            get_panel_consensus_votes_dropped_messages_by_reason(),
            get_panel_consensus_proposals_dropped_messages_by_reason(),
        ],
    )
}
