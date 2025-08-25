use apollo_infra::metrics::{
    MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS,
    MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS,
    MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS,
    MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS,
    MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};

use crate::dashboard::{create_request_type_labeled_hist_panels, Panel, PanelType, Row};

// TODO(alonl): Create all of these panels using a macro (and for the other panels as well).
fn get_panel_mempool_p2p_num_connected_peers() -> Panel {
    Panel::from_gauge(&MEMPOOL_P2P_NUM_CONNECTED_PEERS, PanelType::TimeSeries)
}

fn get_panel_mempool_p2p_num_sent_messages() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_NUM_SENT_MESSAGES, PanelType::TimeSeries)
}

fn get_panel_mempool_p2p_num_received_messages() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, PanelType::TimeSeries)
}

fn get_panel_mempool_p2p_broadcasted_batch_size() -> Panel {
    Panel::from_hist(&MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, PanelType::TimeSeries)
}

fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}

fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}

fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}

fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}

fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(&MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}

fn get_panel_remote_number_of_connections() -> Panel {
    Panel::from_gauge(&MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
}

fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(&MEMPOOL_P2P_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}

fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(&MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_local_client_response_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        &MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_remote_client_response_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        &MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_processing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        &MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_queueing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        &MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}

fn get_remote_client_communication_failure_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        &MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
pub(crate) fn get_mempool_p2p_row() -> Row {
    Row::new(
        "MempoolP2p",
        vec![
            get_panel_mempool_p2p_num_connected_peers(),
            get_panel_mempool_p2p_num_sent_messages(),
            get_panel_mempool_p2p_num_received_messages(),
            get_panel_mempool_p2p_broadcasted_batch_size(),
        ],
    )
}

pub(crate) fn get_mempool_p2p_infra_row() -> Row {
    Row::new(
        "MempoolP2pInfra",
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
        .chain(get_local_client_response_times_panels())
        .chain(get_remote_client_response_times_panels())
        .chain(get_remote_client_communication_failure_times_panels())
        .collect::<Vec<_>>(),
    )
}
