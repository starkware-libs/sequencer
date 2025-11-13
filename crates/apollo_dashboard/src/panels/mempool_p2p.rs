use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_NETWORK_EVENTS,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};
use apollo_metrics::metrics::MetricDetails;
use apollo_network::network_manager::metrics::{
    LABEL_NAME_BROADCAST_DROP_REASON,
    LABEL_NAME_EVENT_TYPE,
};

use crate::dashboard::{Panel, PanelType, Row};
use crate::query_builder::{increase, sum_by_label, DisplayMethod, DEFAULT_DURATION};

// TODO(shahak): Properly name and describe these panels.
fn get_panel_mempool_p2p_num_connected_peers() -> Panel {
    Panel::from_gauge(&MEMPOOL_P2P_NUM_CONNECTED_PEERS, PanelType::TimeSeries)
}

fn get_panel_mempool_p2p_num_sent_messages() -> Panel {
    Panel::new(
        "Number of sent messages",
        "Count of the sent p2p messages",
        increase(&MEMPOOL_P2P_NUM_SENT_MESSAGES, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}

fn get_panel_mempool_p2p_num_received_messages() -> Panel {
    Panel::new(
        "Number of received messages",
        "Count of the received p2p messages",
        increase(&MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}

// TODO(shahak): add units.
fn get_panel_mempool_p2p_broadcasted_batch_size() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
        "Mempool P2p Broadcasted Transaction Batch Size",
        "The number of transactions in batches broadcast by the mempool p2p component",
    )
}

// TODO(shahak): Properly name and describe these panels.
fn get_panel_mempool_p2p_network_events_by_type() -> Panel {
    Panel::new(
        MEMPOOL_P2P_NETWORK_EVENTS.get_name(),
        MEMPOOL_P2P_NETWORK_EVENTS.get_description(),
        sum_by_label(&MEMPOOL_P2P_NETWORK_EVENTS, LABEL_NAME_EVENT_TYPE, DisplayMethod::Raw, false),
        PanelType::TimeSeries,
    )
}

fn get_panel_mempool_p2p_dropped_messages_by_reason() -> Panel {
    Panel::new(
        MEMPOOL_P2P_NUM_DROPPED_MESSAGES.get_name(),
        MEMPOOL_P2P_NUM_DROPPED_MESSAGES.get_description(),
        sum_by_label(
            &MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
            LABEL_NAME_BROADCAST_DROP_REASON,
            DisplayMethod::Raw,
            false,
        ),
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
            get_panel_mempool_p2p_network_events_by_type(),
            get_panel_mempool_p2p_dropped_messages_by_reason(),
        ],
    )
}
