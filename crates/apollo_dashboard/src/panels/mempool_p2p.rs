use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_DROPPED_MESSAGE_SIZE,
    MEMPOOL_P2P_NETWORK_EVENTS,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
    MEMPOOL_P2P_PING_LATENCY,
    MEMPOOL_P2P_RECEIVED_MESSAGE_SIZE,
    MEMPOOL_P2P_SENT_MESSAGE_SIZE,
};
use apollo_metrics::metrics::MetricQueryName;
use apollo_network::metrics::{LABEL_NAME_BROADCAST_DROP_REASON, LABEL_NAME_EVENT_TYPE};

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType, Unit};
use crate::query_builder::{increase, sum_by_label, DisplayMethod, DEFAULT_DURATION};

fn get_panel_mempool_p2p_num_connected_peers() -> Panel {
    Panel::new(
        "Number of Connected Peers",
        "The number of connected peers in Mempool P2P",
        MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query("network_manager")
}

fn get_panel_mempool_p2p_num_sent_messages() -> Panel {
    Panel::new(
        "Number of Sent Messages",
        format!("Count of P2P messages sent by the mempool ({DEFAULT_DURATION} window)"),
        increase(&MEMPOOL_P2P_NUM_SENT_MESSAGES, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}

fn get_panel_mempool_p2p_sent_message_size() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_SENT_MESSAGE_SIZE,
        "Mempool P2P Sent Message Size (MB/sec)",
        "The rate of MB per second sent by the mempool p2p component",
    )
    .with_unit(Unit::MB)
}

fn get_panel_mempool_p2p_num_received_messages() -> Panel {
    Panel::new(
        "Number of Received Messages",
        format!("Count of P2P messages received by the mempool ({DEFAULT_DURATION} window)"),
        increase(&MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}

fn get_panel_mempool_p2p_received_message_size() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_RECEIVED_MESSAGE_SIZE,
        "Mempool P2P Received Message Size (MB/sec)",
        "The rate of MB per second received by the mempool p2p component",
    )
    .with_unit(Unit::MB)
}

fn get_panel_mempool_p2p_broadcasted_batch_size() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
        "Mempool P2P Broadcasted Transaction Batch Size",
        "The number of transactions in batches broadcast by the mempool p2p component",
    )
    .with_unit(Unit::Short)
}

fn get_panel_mempool_p2p_network_events_by_type() -> Panel {
    Panel::new(
        "Network Events by Type",
        "Network events received by mempool p2p, grouped by event type",
        sum_by_label(&MEMPOOL_P2P_NETWORK_EVENTS, LABEL_NAME_EVENT_TYPE, DisplayMethod::Raw, false),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}

fn get_panel_mempool_p2p_dropped_messages_by_reason() -> Panel {
    Panel::new(
        "Dropped Messages by Reason",
        "Mempool p2p messages dropped, grouped by reason",
        sum_by_label(
            &MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
            LABEL_NAME_BROADCAST_DROP_REASON,
            DisplayMethod::Raw,
            false,
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}

fn get_panel_mempool_p2p_dropped_message_size() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_DROPPED_MESSAGE_SIZE,
        "Mempool P2P Dropped Message Size (MB/sec)",
        "The rate of MB per second dropped by the mempool p2p component",
    )
    .with_unit(Unit::MB)
}

fn get_panel_mempool_p2p_ping_latency() -> Panel {
    Panel::from_hist(
        &MEMPOOL_P2P_PING_LATENCY,
        "Ping Latency",
        "The ping latency distribution for mempool p2p connections",
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_mempool_p2p_row() -> Row {
    Row::new(
        "Mempool P2P",
        vec![
            get_panel_mempool_p2p_num_connected_peers(),
            get_panel_mempool_p2p_num_sent_messages(),
            get_panel_mempool_p2p_sent_message_size(),
            get_panel_mempool_p2p_num_received_messages(),
            get_panel_mempool_p2p_received_message_size(),
            get_panel_mempool_p2p_broadcasted_batch_size(),
            get_panel_mempool_p2p_network_events_by_type(),
            get_panel_mempool_p2p_dropped_messages_by_reason(),
            get_panel_mempool_p2p_dropped_message_size(),
            get_panel_mempool_p2p_ping_latency(),
        ],
    )
}
