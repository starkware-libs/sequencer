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
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_TYPE: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
    GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!(
        "sum  by ({}) ({}) ",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    PanelType::Stat,
);

pub(crate) const PANEL_GATEWAY_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_LOCAL_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(GATEWAY_LOCAL_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_VALID_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(GATEWAY_LOCAL_QUEUE_DEPTH, PanelType::Graph);
pub(crate) const PANEL_GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS: Panel =
    Panel::from_hist(GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::Graph);

pub(crate) const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_SOURCE: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
    GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!(
        "sum  by ({}) ({}) ",
        LABEL_NAME_SOURCE,
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    PanelType::Stat,
);

pub(crate) const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_RATE: Panel = Panel::new(
    "gateway_transactions_received_rate (TPS)",
    "The rate of transactions received by the gateway during the last 20 minutes",
    formatcp!(
        "sum(rate({}[20m])) or vector(0)",
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    PanelType::Graph,
);

pub(crate) const PANEL_GATEWAY_ADD_TX_LATENCY: Panel = Panel::new(
    GATEWAY_ADD_TX_LATENCY.get_name(),
    GATEWAY_ADD_TX_LATENCY.get_description(),
    // TODO(Tsabary): revisit this panel, it used to be defined with "avg_over_time({}[2m])".
    GATEWAY_ADD_TX_LATENCY.get_name_with_filter(),
    PanelType::Graph,
);

pub(crate) const PANEL_GATEWAY_VALIDATE_TX_LATENCY: Panel = Panel::new(
    GATEWAY_VALIDATE_TX_LATENCY.get_name(),
    GATEWAY_VALIDATE_TX_LATENCY.get_description(),
    // TODO(Tsabary): revisit this panel, it used to be defined with "avg_over_time({}[2m])".
    GATEWAY_VALIDATE_TX_LATENCY.get_name_with_filter(),
    PanelType::Graph,
);

pub(crate) const PANEL_GATEWAY_TRANSACTIONS_FAILED: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_FAILED.get_name(),
    GATEWAY_TRANSACTIONS_FAILED.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_FAILED.get_name_with_filter()
    ),
    PanelType::Stat,
);

pub(crate) const PANEL_GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name(),
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name_with_filter()
    ),
    PanelType::Stat,
);
