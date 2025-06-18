use apollo_gateway::metrics::{
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_TRANSACTIONS_FAILED,
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    GATEWAY_VALIDATE_TX_LATENCY,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE as GATEWAY_LABEL_NAME_TX_TYPE,
};
use apollo_infra::metrics::{
    GATEWAY_LOCAL_MSGS_PROCESSED,
    GATEWAY_LOCAL_MSGS_RECEIVED,
    GATEWAY_LOCAL_QUEUE_DEPTH,
    GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS,
    GATEWAY_REMOTE_MSGS_PROCESSED,
    GATEWAY_REMOTE_MSGS_RECEIVED,
    GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_gateway_transactions_received_by_type() -> Panel {
    Panel::new(
        GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
        GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
        vec![format!(
            "sum  by ({}) ({}) ",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

fn get_panel_gateway_local_msgs_received() -> Panel {
    Panel::from_counter(GATEWAY_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_gateway_local_msgs_processed() -> Panel {
    Panel::from_counter(GATEWAY_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_gateway_remote_msgs_received() -> Panel {
    Panel::from_counter(GATEWAY_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_gateway_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(GATEWAY_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_gateway_remote_msgs_processed() -> Panel {
    Panel::from_counter(GATEWAY_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_gateway_local_queue_depth() -> Panel {
    Panel::from_gauge(GATEWAY_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_gateway_remote_client_send_attempts() -> Panel {
    Panel::from_hist(GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}

fn get_panel_gateway_transactions_received_by_source() -> Panel {
    Panel::new(
        GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
        GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
        vec![format!(
            "sum  by ({}) ({}) ",
            LABEL_NAME_SOURCE,
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

fn get_panel_gateway_transactions_received_rate() -> Panel {
    Panel::new(
        "gateway_transactions_received_rate (TPS)",
        "The rate of transactions received by the gateway during the last 20 minutes",
        vec![format!(
            "sum(rate({}[20m])) or vector(0)",
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_gateway_add_tx_latency() -> Panel {
    Panel::from_hist(GATEWAY_ADD_TX_LATENCY, PanelType::TimeSeries)
}

fn get_panel_gateway_validate_tx_latency() -> Panel {
    Panel::from_hist(GATEWAY_VALIDATE_TX_LATENCY, PanelType::TimeSeries)
}

fn get_panel_gateway_transactions_failed() -> Panel {
    Panel::new(
        GATEWAY_TRANSACTIONS_FAILED.get_name(),
        GATEWAY_TRANSACTIONS_FAILED.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_FAILED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

fn get_panel_gateway_transactions_sent_to_mempool() -> Panel {
    Panel::new(
        GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name(),
        GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

pub(crate) fn get_gateway_row() -> Row {
    Row::new(
        "Gateway",
        vec![
            get_panel_gateway_transactions_received_by_type(),
            get_panel_gateway_transactions_received_by_source(),
            get_panel_gateway_transactions_received_rate(),
            get_panel_gateway_add_tx_latency(),
            get_panel_gateway_validate_tx_latency(),
            get_panel_gateway_transactions_failed(),
            get_panel_gateway_transactions_sent_to_mempool(),
        ],
    )
}

pub(crate) fn get_gateway_infra_row() -> Row {
    Row::new(
        "Gateway Infra",
        vec![
            get_panel_gateway_local_msgs_received(),
            get_panel_gateway_local_msgs_processed(),
            get_panel_gateway_local_queue_depth(),
            get_panel_gateway_remote_msgs_received(),
            get_panel_gateway_remote_valid_msgs_received(),
            get_panel_gateway_remote_msgs_processed(),
            get_panel_gateway_remote_client_send_attempts(),
        ],
    )
}
