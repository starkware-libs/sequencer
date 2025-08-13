use apollo_infra::metrics::{
    MEMPOOL_LOCAL_MSGS_PROCESSED,
    MEMPOOL_LOCAL_MSGS_RECEIVED,
    MEMPOOL_LOCAL_QUEUE_DEPTH,
    MEMPOOL_PROCESSING_TIMES,
    MEMPOOL_QUEUEING_TIMES,
    MEMPOOL_REMOTE_CLIENT_SEND_ATTEMPTS,
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
    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(MEMPOOL_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(MEMPOOL_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_panel_processing_times() -> Panel {
    Panel::from_hist(MEMPOOL_PROCESSING_TIMES, PanelType::TimeSeries)
}
fn get_panel_queueing_times() -> Panel {
    Panel::from_hist(MEMPOOL_QUEUEING_TIMES, PanelType::TimeSeries)
}

fn get_panel_mempool_transactions_received() -> Panel {
    Panel::new(
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name(),
        MEMPOOL_TRANSACTIONS_RECEIVED.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            MEMPOOL_LABEL_NAME_TX_TYPE,
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_mempool_transactions_received_rate() -> Panel {
    Panel::new(
        "mempool_transactions_received_rate (TPS)",
        "The rate of transactions received by the mempool during the last 20 minutes",
        vec![format!(
            "sum(rate({}[20m])) or vector(0)",
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transactions_committed() -> Panel {
    Panel::from_counter(MEMPOOL_TRANSACTIONS_COMMITTED, PanelType::Stat)
}
fn get_panel_mempool_transactions_dropped() -> Panel {
    Panel::new(
        MEMPOOL_TRANSACTIONS_DROPPED.get_name(),
        MEMPOOL_TRANSACTIONS_DROPPED.get_description(),
        vec![format!(
            "sum  by ({}) ({})",
            LABEL_NAME_DROP_REASON,
            MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_mempool_pool_size() -> Panel {
    Panel::new(
        MEMPOOL_POOL_SIZE.get_name(),
        "The average size of the pool",
        vec![format!("avg_over_time({}[2m])", MEMPOOL_POOL_SIZE.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_priority_queue_size() -> Panel {
    Panel::new(
        MEMPOOL_PRIORITY_QUEUE_SIZE.get_name(),
        "The average size of the priority queue",
        vec![format!("avg_over_time({}[2m])", MEMPOOL_PRIORITY_QUEUE_SIZE.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_pending_queue_size() -> Panel {
    Panel::new(
        MEMPOOL_PENDING_QUEUE_SIZE.get_name(),
        "The average size of the pending queue",
        vec![format!("avg_over_time({}[2m])", MEMPOOL_PENDING_QUEUE_SIZE.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_total_size_in_bytes() -> Panel {
    Panel::new(
        MEMPOOL_TOTAL_SIZE_BYTES.get_name(),
        "The average total transaction size in bytes over time in the mempool",
        vec![format!("avg_over_time({}[2m])", MEMPOOL_TOTAL_SIZE_BYTES.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_get_txs_size() -> Panel {
    Panel::new(
        MEMPOOL_GET_TXS_SIZE.get_name(),
        "The average size of the get_txs",
        vec![format!("avg_over_time({}[2m])", MEMPOOL_GET_TXS_SIZE.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_delayed_declares_size() -> Panel {
    Panel::new(
        MEMPOOL_DELAYED_DECLARES_SIZE.get_name(),
        "The average number of delayed declare transactions",
        vec![format!(
            "avg_over_time({}[2m])",
            MEMPOOL_DELAYED_DECLARES_SIZE.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transaction_time_spent() -> Panel {
    Panel::from_hist(TRANSACTION_TIME_SPENT_IN_MEMPOOL, PanelType::TimeSeries)
}
fn get_panel_mempool_transaction_time_spent_until_committed() -> Panel {
    Panel::from_hist(TRANSACTION_TIME_SPENT_UNTIL_COMMITTED, PanelType::TimeSeries)
}

pub(crate) fn get_mempool_row() -> Row {
    Row::new(
        "Mempool",
        vec![
            get_panel_mempool_transactions_received(),
            get_panel_mempool_transactions_received_rate(),
            get_panel_mempool_transactions_dropped(),
            get_panel_mempool_transactions_committed(),
            get_panel_mempool_pool_size(),
            get_panel_mempool_priority_queue_size(),
            get_panel_mempool_pending_queue_size(),
            get_panel_mempool_total_size_in_bytes(),
            get_panel_mempool_get_txs_size(),
            get_panel_mempool_delayed_declares_size(),
            get_panel_mempool_transaction_time_spent(),
            get_panel_mempool_transaction_time_spent_until_committed(),
        ],
    )
}

pub(crate) fn get_mempool_infra_row() -> Row {
    Row::new(
        "Mempool Infra",
        vec![
            get_panel_local_msgs_received(),
            get_panel_local_msgs_processed(),
            get_panel_local_queue_depth(),
            get_panel_remote_msgs_received(),
            get_panel_remote_valid_msgs_received(),
            get_panel_remote_msgs_processed(),
            get_panel_remote_client_send_attempts(),
            get_panel_processing_times(),
            get_panel_queueing_times(),
        ],
    )
}
