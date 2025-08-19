use apollo_infra::metrics::{
    STATE_SYNC_LOCAL_MSGS_PROCESSED,
    STATE_SYNC_LOCAL_MSGS_RECEIVED,
    STATE_SYNC_LOCAL_QUEUE_DEPTH,
    STATE_SYNC_REMOTE_CLIENT_SEND_ATTEMPTS,
    STATE_SYNC_REMOTE_MSGS_PROCESSED,
    STATE_SYNC_REMOTE_MSGS_RECEIVED,
    STATE_SYNC_REMOTE_NUMBER_OF_CONNECTIONS,
    STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    P2P_SYNC_NUM_CONNECTED_PEERS,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_HEADER_MARKER,
    STATE_SYNC_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    STATE_SYNC_LABELED_PROCESSING_TIMES_SECS,
    STATE_SYNC_LABELED_QUEUEING_TIMES_SECS,
    STATE_SYNC_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_REVERTED_TRANSACTIONS,
    STATE_SYNC_STATE_MARKER,
};

use crate::dashboard::{create_request_type_labeled_hist_panels, Panel, PanelType, Row};

fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(STATE_SYNC_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(STATE_SYNC_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_panel_remote_number_of_connections() -> Panel {
    Panel::from_gauge(STATE_SYNC_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
}
fn get_processing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        STATE_SYNC_LABELED_PROCESSING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_queueing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        STATE_SYNC_LABELED_QUEUEING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_panel_local_client_response_times() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        STATE_SYNC_LABELED_LOCAL_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_panel_remote_client_response_times() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        STATE_SYNC_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}

fn get_panel_p2p_sync_num_connected_peers() -> Panel {
    Panel::from_gauge(P2P_SYNC_NUM_CONNECTED_PEERS, PanelType::Stat)
}
fn get_panel_p2p_sync_num_active_inbound_sessions() -> Panel {
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, PanelType::Stat)
}
fn get_panel_p2p_sync_num_active_outbound_sessions() -> Panel {
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, PanelType::Stat)
}
fn get_panel_state_sync_processed_transactions() -> Panel {
    Panel::from_counter(STATE_SYNC_PROCESSED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_state_sync_reverted_transactions() -> Panel {
    Panel::from_counter(STATE_SYNC_REVERTED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_central_sync_central_block_marker() -> Panel {
    Panel::from_gauge(CENTRAL_SYNC_CENTRAL_BLOCK_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_body_marker() -> Panel {
    Panel::from_gauge(STATE_SYNC_BODY_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_class_manager_marker() -> Panel {
    Panel::from_gauge(STATE_SYNC_CLASS_MANAGER_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_header_marker() -> Panel {
    Panel::from_gauge(STATE_SYNC_HEADER_MARKER, PanelType::Stat)
}
fn get_panel_state_sync_state_marker() -> Panel {
    Panel::from_gauge(STATE_SYNC_STATE_MARKER, PanelType::Stat)
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

pub(crate) fn get_state_sync_infra_row() -> Row {
    Row::new(
        "StateSyncInfra",
        vec![
            get_panel_local_msgs_received(),
            get_panel_local_msgs_processed(),
            get_panel_local_queue_depth(),
            get_panel_remote_msgs_received(),
            get_panel_remote_valid_msgs_received(),
            get_panel_remote_msgs_processed(),
            get_panel_remote_number_of_connections(),
            get_panel_remote_client_send_attempts(),
        ]
        .into_iter()
        .chain(get_processing_times_panels())
        .chain(get_queueing_times_panels())
        .chain(get_panel_local_client_response_times())
        .chain(get_panel_remote_client_response_times())
        .collect::<Vec<_>>(),
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
