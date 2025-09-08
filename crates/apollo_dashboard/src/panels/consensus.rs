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
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use apollo_consensus_orchestrator::metrics::{
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_DATA_GAS_MISMATCH,
    CONSENSUS_L1_GAS_MISMATCH,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
    LABEL_CENDE_FAILURE_REASON,
};
use apollo_state_sync_metrics::metrics::STATE_SYNC_CLASS_MANAGER_MARKER;

use crate::dashboard::{Panel, PanelType, Row, Unit, HISTOGRAM_QUANTILES, HISTOGRAM_TIME_RANGE};

fn get_panel_consensus_block_number() -> Panel {
    Panel::new(
        "Consensus Height",
        "The block height the node is currently working on",
        vec![CONSENSUS_BLOCK_NUMBER.get_name_with_filter().to_string()],
        PanelType::Stat,
    )
}
fn get_panel_consensus_block_number_diff_between_nodes() -> Panel {
    Panel::new(
        "Height Diff Between Nodes",
        "The difference between the highest and lowest consensus heights between nodes",
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
        "Consensus Height Diff From Sync",
        "The difference between the consensus height and the sync height",
        vec![format!(
            "({} - {})",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_consensus_round() -> Panel {
    Panel::new(
        "Consensus Round",
        "The round the node is currently working on",
        vec![CONSENSUS_ROUND.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_round_avg() -> Panel {
    Panel::new(
        "Average Consensus Round",
        "Average consensus round (10m)",
        vec![format!("avg_over_time({}[10m])", CONSENSUS_ROUND.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_block_time_avg() -> Panel {
    Panel::new(
        "Average Block Time",
        "Average block time (10m)",
        vec![format!("1 / rate({}[10m])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter())],
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_consensus_round_above_zero() -> Panel {
    Panel::new(
        "Consensus Rounds Above Zero",
        "The number of times the consensus round has increased above zero",
        vec![format!("increase({}[10m])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_max_cached_block_number() -> Panel {
    Panel::from_gauge(&CONSENSUS_MAX_CACHED_BLOCK_NUMBER, PanelType::TimeSeries)
}
fn get_panel_consensus_cached_votes() -> Panel {
    Panel::from_gauge(&CONSENSUS_CACHED_VOTES, PanelType::TimeSeries)
}
fn get_panel_consensus_decisions_reached_by_consensus() -> Panel {
    Panel::new(
        "Decisions Reached By Consensus",
        "The number of decisions reached by way of consensus",
        vec![format!(
            "increase({}[10m])",
            CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_decisions_reached_by_sync() -> Panel {
    Panel::new(
        "Decisions Reached By Sync",
        "The number of decisions reached by way of sync",
        vec![format!(
            "increase({}[10m])",
            CONSENSUS_DECISIONS_REACHED_BY_SYNC.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_inbound_stream_started() -> Panel {
    Panel::from_counter(&CONSENSUS_INBOUND_STREAM_STARTED, PanelType::TimeSeries)
}
fn get_panel_consensus_inbound_stream_evicted() -> Panel {
    Panel::from_counter(&CONSENSUS_INBOUND_STREAM_EVICTED, PanelType::TimeSeries)
}
fn get_panel_consensus_inbound_stream_finished() -> Panel {
    Panel::from_counter(&CONSENSUS_INBOUND_STREAM_FINISHED, PanelType::TimeSeries)
}
fn get_panel_consensus_outbound_stream_started() -> Panel {
    Panel::from_counter(&CONSENSUS_OUTBOUND_STREAM_STARTED, PanelType::TimeSeries)
}
fn get_panel_consensus_outbound_stream_finished() -> Panel {
    Panel::from_counter(&CONSENSUS_OUTBOUND_STREAM_FINISHED, PanelType::TimeSeries)
}
fn get_panel_consensus_proposals_received() -> Panel {
    Panel::new(
        "Proposals Received",
        "The number of proposals received from the network",
        vec![format!("increase({}[10m])", CONSENSUS_PROPOSALS_RECEIVED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_proposals_validated() -> Panel {
    Panel::new(
        "Proposal Validation Success",
        "The number of proposals received and validated successfully",
        vec![format!("increase({}[10m])", CONSENSUS_PROPOSALS_VALIDATED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_proposals_invalid() -> Panel {
    Panel::new(
        "Proposal Validation Failed",
        "The number of proposals received and failed validation",
        vec![format!("increase({}[10m])", CONSENSUS_PROPOSALS_INVALID.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_proposals_valid_init() -> Panel {
    Panel::new(
        "Proposals Received - Valid Init",
        "The number of proposals received with a valid init",
        vec![format!("increase({}[10m])", CONSENSUS_PROPOSALS_VALID_INIT.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_l1_gas_mismatch() -> Panel {
    Panel::new(
        "Proposal Received - L1 Gas Mismatch",
        "The number of proposals received with a L1 gas mismatch",
        vec![format!("increase({}[10m])", CONSENSUS_L1_GAS_MISMATCH.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_l1_data_gas_mismatch() -> Panel {
    Panel::new(
        "Proposal Received - L1 Data Gas Mismatch",
        "The number of proposals received with a L1 data gas mismatch",
        vec![format!("increase({}[10m])", CONSENSUS_L1_DATA_GAS_MISMATCH.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_build_proposal_total() -> Panel {
    Panel::new(
        "Proposal Build",
        "The number of proposals that started building",
        vec![format!("increase({}[10m])", CONSENSUS_BUILD_PROPOSAL_TOTAL.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_build_proposal_failed() -> Panel {
    Panel::new(
        "Proposal Build Failed",
        "The number of proposals that failed to be built",
        vec![format!("increase({}[10m])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_reproposals() -> Panel {
    Panel::from_counter(&CONSENSUS_REPROPOSALS, PanelType::TimeSeries)
}
fn get_panel_consensus_new_value_locks() -> Panel {
    Panel::from_counter(&CONSENSUS_NEW_VALUE_LOCKS, PanelType::TimeSeries)
}
fn get_panel_consensus_held_locks() -> Panel {
    Panel::from_counter(&CONSENSUS_HELD_LOCKS, PanelType::TimeSeries)
}
fn get_panel_consensus_timeouts_by_type() -> Panel {
    Panel::new(
        "Consensus Timeouts By Reason",
        CONSENSUS_TIMEOUTS.get_description(),
        vec![format!(
            "sum by ({}) (increase({}[10m]))",
            LABEL_NAME_TIMEOUT_REASON,
            CONSENSUS_TIMEOUTS.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_num_batches_in_proposal() -> Panel {
    Panel::new(
        "Number of Batches Received in Proposal",
        "The number of transaction batches received in a valid proposal",
        vec![CONSENSUS_NUM_BATCHES_IN_PROPOSAL.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_num_txs_in_proposal() -> Panel {
    Panel::new(
        "Number of Transactions in Proposal Received",
        "The total number of individual transactions in a valid proposal received",
        vec![CONSENSUS_NUM_TXS_IN_PROPOSAL.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_consensus_l2_gas_price() -> Panel {
    // TODO(Dafna): Better presentation of the price units.
    Panel::from_gauge(&CONSENSUS_L2_GAS_PRICE, PanelType::TimeSeries)
}
fn get_panel_consensus_num_connected_peers() -> Panel {
    Panel::from_gauge(&CONSENSUS_NUM_CONNECTED_PEERS, PanelType::TimeSeries)
}
fn get_panel_consensus_votes_num_sent_messages() -> Panel {
    Panel::from_counter(&CONSENSUS_VOTES_NUM_SENT_MESSAGES, PanelType::TimeSeries)
}
fn get_panel_consensus_votes_num_received_messages() -> Panel {
    Panel::from_counter(&CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, PanelType::TimeSeries)
}
fn get_panel_consensus_proposals_num_sent_messages() -> Panel {
    Panel::from_counter(&CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, PanelType::TimeSeries)
}
fn get_panel_consensus_proposals_num_received_messages() -> Panel {
    Panel::from_counter(&CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, PanelType::TimeSeries)
}
fn get_panel_consensus_conflicting_votes() -> Panel {
    Panel::from_counter(&CONSENSUS_CONFLICTING_VOTES, PanelType::TimeSeries)
}
fn get_panel_cende_last_prepared_blob_block_number() -> Panel {
    Panel::new(
        "Last Prepared Blob Block Number",
        "The block number that is ready to be sent to Cende in the next height",
        vec![CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.get_name_with_filter().to_string()],
        PanelType::Stat,
    )
}
fn get_panel_cende_prepare_blob_for_next_height_latency() -> Panel {
    Panel::new(
        "Prepare Blob for Next Height Latency",
        "The time it takes to prepare the blob for the next height",
        HISTOGRAM_QUANTILES
            .iter()
            .map(|q| {
                format!(
                    "histogram_quantile({q:.2}, sum by (le) (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_name_with_filter(),
                )
            })
            .collect(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_cende_write_prev_height_blob_latency() -> Panel {
    Panel::new(
        "Write Blob Latency",
        "The time it takes to write the blob to Cende",
        // TODO(Dafna): add an helper function to generate a vector of histogram expressions, to be
        // used everywhere
        HISTOGRAM_QUANTILES
            .iter()
            .map(|q| {
                format!(
                    "histogram_quantile({q:.2}, sum by (le) (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_with_filter(),
                )
            })
            .collect(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_cende_write_blob_success() -> Panel {
    Panel::new(
        "Write Blob Success",
        "The number of successful blob writes to Cende",
        vec![format!("increase({}[10m])", CENDE_WRITE_BLOB_SUCCESS.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_cende_write_blob_failure() -> Panel {
    Panel::new(
        "Write Blob Failure by Reason",
        "The number of failed blob writes to Cende",
        vec![format!(
            "sum by ({}) (increase({}[10m]))",
            LABEL_CENDE_FAILURE_REASON,
            CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()
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
            get_panel_consensus_block_time_avg(),
            get_panel_consensus_round_above_zero(),
            get_panel_consensus_block_number_diff_between_nodes(),
            get_panel_consensus_block_number_diff_from_sync(),
            get_panel_consensus_decisions_reached_by_consensus(),
            get_panel_consensus_decisions_reached_by_sync(),
            get_panel_consensus_proposals_received(),
            get_panel_consensus_proposals_validated(),
            get_panel_consensus_proposals_invalid(),
            get_panel_consensus_proposals_valid_init(),
            get_panel_consensus_l1_data_gas_mismatch(),
            get_panel_consensus_l1_gas_mismatch(),
            get_panel_consensus_build_proposal_total(),
            get_panel_consensus_build_proposal_failed(),
            get_panel_consensus_timeouts_by_type(),
            get_panel_consensus_num_batches_in_proposal(),
            get_panel_consensus_num_txs_in_proposal(),
            get_panel_consensus_l2_gas_price(),
            // TODO(Dafna): Can we remove these panels below?
            get_panel_consensus_max_cached_block_number(),
            get_panel_consensus_cached_votes(),
            get_panel_consensus_reproposals(),
            get_panel_consensus_new_value_locks(),
            get_panel_consensus_held_locks(),
            get_panel_consensus_inbound_stream_started(),
            get_panel_consensus_inbound_stream_evicted(),
            get_panel_consensus_inbound_stream_finished(),
            get_panel_consensus_outbound_stream_started(),
            get_panel_consensus_outbound_stream_finished(),
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
            get_panel_cende_prepare_blob_for_next_height_latency(),
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
        ],
    )
}
