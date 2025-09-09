use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    P2P_SYNC_NUM_CONNECTED_PEERS,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_HEADER_MARKER,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_REVERTED_TRANSACTIONS,
    STATE_SYNC_STATE_MARKER,
};

use crate::dashboard::{Panel, PanelType, Row};

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

fn get_panel_state_sync_processed_transactions() -> Panel {
    Panel::from_counter(&STATE_SYNC_PROCESSED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_state_sync_reverted_transactions() -> Panel {
    Panel::from_counter(&STATE_SYNC_REVERTED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_central_sync_central_block_marker() -> Panel {
    Panel::from_gauge(&CENTRAL_SYNC_CENTRAL_BLOCK_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_body_marker() -> Panel {
    Panel::from_gauge(&STATE_SYNC_BODY_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_class_manager_marker() -> Panel {
    Panel::from_gauge(&STATE_SYNC_CLASS_MANAGER_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_header_marker() -> Panel {
    Panel::from_gauge(&STATE_SYNC_HEADER_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_state_marker() -> Panel {
    Panel::from_gauge(&STATE_SYNC_STATE_MARKER, PanelType::Stat)
}

pub(crate) fn get_state_sync_row() -> Row {
    Row::new(
        "State Sync",
        vec![
            get_panel_state_sync_processed_transactions(),
            get_panel_state_sync_reverted_transactions(),
            get_panel_central_sync_central_block_marker(),
            get_panel_state_sync_body_marker(),
            get_panel_state_sync_class_manager_marker(),
            get_panel_state_sync_header_marker(),
            get_panel_state_sync_state_marker(),
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
