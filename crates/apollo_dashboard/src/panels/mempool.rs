use apollo_infra::metrics::{
    MEMPOOL_LOCAL_MSGS_PROCESSED,
    MEMPOOL_LOCAL_MSGS_RECEIVED,
    MEMPOOL_LOCAL_QUEUE_DEPTH,
    MEMPOOL_REMOTE_MSGS_PROCESSED,
    MEMPOOL_REMOTE_MSGS_RECEIVED,
    MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool::metrics::{
    LABEL_NAME_DROP_REASON,
    LABEL_NAME_TX_TYPE as MEMPOOL_LABEL_NAME_TX_TYPE,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_TOTAL_SIZE_BYTES,
    MEMPOOL_TRANSACTIONS_COMMITTED,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
    TRANSACTION_TIME_SPENT_IN_MEMPOOL,
};
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_MEMPOOL_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(MEMPOOL_LOCAL_QUEUE_DEPTH, PanelType::Stat);

pub(crate) const PANEL_MEMPOOL_TRANSACTIONS_RECEIVED: Panel = Panel::new(
    MEMPOOL_TRANSACTIONS_RECEIVED.get_name(),
    MEMPOOL_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        MEMPOOL_LABEL_NAME_TX_TYPE,
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    PanelType::Stat,
);

pub(crate) const PANEL_MEMPOOL_TRANSACTIONS_RECEIVED_RATE: Panel = Panel::new(
    "mempool_transactions_received_rate (TPS)",
    "The rate of transactions received by the mempool during the last 20 minutes",
    formatcp!(
        "sum(rate({}[20m])) or vector(0)",
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_TRANSACTIONS_COMMITTED: Panel =
    Panel::from_counter(MEMPOOL_TRANSACTIONS_COMMITTED, PanelType::Stat);

pub(crate) const PANEL_MEMPOOL_TRANSACTIONS_DROPPED: Panel = Panel::new(
    MEMPOOL_TRANSACTIONS_DROPPED.get_name(),
    MEMPOOL_TRANSACTIONS_DROPPED.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        LABEL_NAME_DROP_REASON,
        MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter()
    ),
    PanelType::Stat,
);

pub(crate) const PANEL_MEMPOOL_POOL_SIZE: Panel = Panel::new(
    MEMPOOL_POOL_SIZE.get_name(),
    "The average size of the pool",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_POOL_SIZE.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_PRIORITY_QUEUE_SIZE: Panel = Panel::new(
    MEMPOOL_PRIORITY_QUEUE_SIZE.get_name(),
    "The average size of the priority queue",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_PRIORITY_QUEUE_SIZE.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_PENDING_QUEUE_SIZE: Panel = Panel::new(
    MEMPOOL_PENDING_QUEUE_SIZE.get_name(),
    "The average size of the pending queue",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_PENDING_QUEUE_SIZE.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_TOTAL_SIZE_IN_BYTES: Panel = Panel::new(
    MEMPOOL_TOTAL_SIZE_BYTES.get_name(),
    "The average total transaction size in bytes over time in the mempool",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_TOTAL_SIZE_BYTES.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_GET_TXS_SIZE: Panel = Panel::new(
    MEMPOOL_GET_TXS_SIZE.get_name(),
    "The average size of the get_txs",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_GET_TXS_SIZE.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_DELAYED_DECLARES_SIZE: Panel = Panel::new(
    MEMPOOL_DELAYED_DECLARES_SIZE.get_name(),
    "The average number of delayed declare transactions",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_DELAYED_DECLARES_SIZE.get_name_with_filter()),
    PanelType::Graph,
);

pub(crate) const PANEL_MEMPOOL_TRANSACTION_TIME_SPENT: Panel = Panel::new(
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_name(),
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_description(),
    formatcp!("avg_over_time({}[2m])", TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_name_with_filter()),
    PanelType::Graph,
);
