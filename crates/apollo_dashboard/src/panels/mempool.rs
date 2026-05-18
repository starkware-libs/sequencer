use apollo_mempool::metrics::{
    LABEL_NAME_DROP_REASON,
    MEMPOOL_ACCOUNTS_WITH_GAP,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_STUCK_TXS,
    MEMPOOL_TOTAL_SIZE_BYTES,
    MEMPOOL_TRANSACTIONS_COMMITTED,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
    TRANSACTION_TIME_SPENT_UNTIL_BATCHED,
    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::Row;
use crate::panel::{traffic_light_thresholds, Panel, PanelType, Unit};
use crate::query_builder::{
    increase,
    sum_by_label,
    DisplayMethod,
    DEFAULT_DURATION,
    RANGE_DURATION,
};

fn get_panel_mempool_transactions_received_rate() -> Panel {
    Panel::new(
        "Mempool Transactions Received Rate (TPS)",
        "The rate of transactions received by the mempool (1m window)",
        format!(
            "sum(rate({}[1m])) or vector(0)",
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
    .with_log_query("Adding transaction to mempool")
}
fn get_panel_mempool_transactions_committed() -> Panel {
    Panel::new(
        "Transactions Committed",
        format!("Number of transactions committed to a block ({DEFAULT_DURATION} window)"),
        increase(&MEMPOOL_TRANSACTIONS_COMMITTED, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transactions_dropped() -> Panel {
    Panel::new(
        "Dropped Transactions by Reason",
        "Number of transactions dropped from the mempool by reason (over the selected time range)",
        sum_by_label(
            &MEMPOOL_TRANSACTIONS_DROPPED,
            LABEL_NAME_DROP_REASON,
            DisplayMethod::Increase(RANGE_DURATION),
            false,
        ),
        PanelType::Stat,
    )
}
fn get_panel_mempool_pool_size() -> Panel {
    Panel::new(
        "Pool Size (Num TXs)",
        "Number of all the transactions in the mempool",
        MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}
fn get_panel_mempool_priority_queue_size() -> Panel {
    Panel::new(
        "Prioritized Transactions",
        "Number of transactions prioritized for batching",
        MEMPOOL_PRIORITY_QUEUE_SIZE.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}
fn get_panel_mempool_pending_queue_size() -> Panel {
    Panel::new(
        "Pending Transactions",
        "Number of transactions eligible for batching but below the gas price threshold",
        MEMPOOL_PENDING_QUEUE_SIZE.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
}
fn get_panel_mempool_total_size_in_bytes() -> Panel {
    Panel::new(
        "Mempool Size (Data)",
        "Size of the transactions in the mempool",
        MEMPOOL_TOTAL_SIZE_BYTES.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Bytes)
}
fn get_panel_mempool_delayed_declares_size() -> Panel {
    Panel::new(
        "Delayed Declare Transactions",
        "Number of delayed declare transactions",
        MEMPOOL_DELAYED_DECLARES_SIZE.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_accounts_with_gap() -> Panel {
    Panel::new(
        "Accounts With Nonce Gap",
        "Number of accounts whose lowest pool nonce exceeds the account nonce, making them unable \
         to provide any transaction for batching",
        MEMPOOL_ACCOUNTS_WITH_GAP.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
    .with_absolute_thresholds(traffic_light_thresholds(3.0, 10.0))
    .with_log_query("has a nonce gap")
}
fn get_panel_mempool_stuck_txs() -> Panel {
    Panel::new(
        "Stuck Transactions (Nonce Gap)",
        "Number of transactions in the pool belonging to accounts whose lowest pool nonce exceeds \
         the account nonce",
        MEMPOOL_STUCK_TXS.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Short)
    .with_absolute_thresholds(traffic_light_thresholds(20.0, 100.0))
    .with_log_query("has a nonce gap")
}
fn get_panel_mempool_transaction_time_spent_until_batched() -> Panel {
    Panel::from_hist(
        &TRANSACTION_TIME_SPENT_UNTIL_BATCHED,
        "Transaction Time Spent in Mempool Until Batched",
        "The time a transaction spends in the mempool until it is batched (5m window)",
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_mempool_transaction_time_spent_until_committed() -> Panel {
    Panel::from_hist(
        &TRANSACTION_TIME_SPENT_UNTIL_COMMITTED,
        "Transaction Time Spent in Mempool Until Committed",
        "The time a transaction spends in the mempool until it is committed (5m window)",
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_mempool_row() -> Row {
    Row::new(
        "Mempool",
        vec![
            get_panel_mempool_transactions_received_rate(),
            get_panel_mempool_transactions_committed(),
            get_panel_mempool_transactions_dropped(),
            get_panel_mempool_pool_size(),
            get_panel_mempool_total_size_in_bytes(),
            get_panel_mempool_priority_queue_size(),
            get_panel_mempool_pending_queue_size(),
            get_panel_mempool_delayed_declares_size(),
            get_panel_mempool_accounts_with_gap(),
            get_panel_mempool_stuck_txs(),
            get_panel_mempool_transaction_time_spent_until_batched(),
            get_panel_mempool_transaction_time_spent_until_committed(),
        ],
    )
}
