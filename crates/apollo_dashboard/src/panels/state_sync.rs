use apollo_metrics::MetricCommon;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    P2P_SYNC_NUM_CONNECTED_PEERS,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_HEADER_LATENCY_SEC,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

// P2P panels

fn get_panel_p2p_sync_num_connected_peers() -> Panel {
    Panel::from_gauge(&P2P_SYNC_NUM_CONNECTED_PEERS, PanelType::Stat)
}
fn get_panel_p2p_sync_num_active_inbound_sessions() -> Panel {
    Panel::from_gauge(&P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, PanelType::Stat)
}
fn get_panel_p2p_sync_num_active_outbound_sessions() -> Panel {
    Panel::from_gauge(&P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, PanelType::Stat)
}

// State Sync panels

fn get_panel_central_sync_central_block_marker() -> Panel {
    Panel::new(
        "Central Block Marker",
        "The first block that Central Starknet hasn't seen yet",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_body_marker() -> Panel {
    Panel::new(
        "State Sync Body Marker",
        "The first block number for which the state sync component does not have a body",
        STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_diff_from_central() -> Panel {
    Panel::new(
        "Sync Diff From Central",
        "The number of blocks that were not fully synced yet",
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
}
fn get_panel_state_sync_new_header_maturity() -> Panel {
    Panel::new(
        "Sync Block Age",
        "The time from a block’s timestamp until its header is synced through the feeder-gateway.",
        STATE_SYNC_HEADER_LATENCY_SEC.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_state_sync_row() -> Row {
    Row::new(
        "State Sync",
        vec![
            get_panel_central_sync_central_block_marker(),
            get_panel_state_sync_body_marker(),
            get_panel_state_sync_diff_from_central(),
            get_panel_state_sync_new_header_maturity(),
        ],
    )
}

pub(crate) fn get_state_sync_p2p_row() -> Row {
    Row::new(
        "StateSyncP2p",
        vec![
            get_panel_p2p_sync_num_connected_peers(),
            get_panel_p2p_sync_num_active_inbound_sessions(),
            get_panel_p2p_sync_num_active_outbound_sessions(),
        ],
    )
}
