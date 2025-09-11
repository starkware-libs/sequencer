use apollo_mempool::metrics::{
    LABEL_NAME_DROP_REASON,
    LABEL_NAME_TX_TYPE as MEMPOOL_LABEL_NAME_TX_TYPE,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_TOTAL_SIZE_BYTES,
    MEMPOOL_TRANSACTIONS_COMMITTED,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED,
};

use crate::dashboard::{Panel, PanelType, Row, Unit, HISTOGRAM_QUANTILES, HISTOGRAM_TIME_RANGE};

fn get_panel_mempool_transactions_received() -> Panel {
    Panel::new(
        "Transactions Received by Type",
        "Number of transactions received by type",
        vec![format!(
            "sum  by ({}) (increase({}[$__range]))",
            MEMPOOL_LABEL_NAME_TX_TYPE,
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_mempool_transactions_received_rate() -> Panel {
    Panel::new(
        "Transactions Received Rate (TPS)",
        "The rate of transactions received by the mempool (1m window)",
        vec![format!(
            "sum(rate({}[1m])) or vector(0)",
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transactions_committed() -> Panel {
    Panel::new(
        "Transactions Committed",
        "Number of transactions committed to a block (10m window)",
        vec![format!("increase({}[10m])", MEMPOOL_TRANSACTIONS_COMMITTED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transactions_dropped() -> Panel {
    Panel::new(
        "Dropped Transactions by Reason",
        "Number of transactions dropped from the mempool by reason",
        vec![format!(
            "sum  by ({}) (increase({}[$__range]))",
            LABEL_NAME_DROP_REASON,
            MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_mempool_pool_size() -> Panel {
    Panel::new(
        "Pool Size (Num TXs)",
        "Number of all the transactions in the mempool",
        vec![MEMPOOL_POOL_SIZE.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_priority_queue_size() -> Panel {
    Panel::new(
        "Prioritized Transactions",
        "Number of transactions prioritized for batching",
        vec![MEMPOOL_PRIORITY_QUEUE_SIZE.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_pending_queue_size() -> Panel {
    Panel::new(
        "Pending Transactions",
        "Number of transactions eligible for batching but below the gas price threshold",
        vec![MEMPOOL_PENDING_QUEUE_SIZE.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_total_size_in_bytes() -> Panel {
    Panel::new(
        "Mempool Size (Data)",
        "Size of the transactions in the mempool",
        vec![MEMPOOL_TOTAL_SIZE_BYTES.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Bytes)
}
fn get_panel_mempool_delayed_declares_size() -> Panel {
    Panel::new(
        "Delayed Declare Transactions",
        "Number of delayed declare transactions",
        vec![MEMPOOL_DELAYED_DECLARES_SIZE.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_mempool_transaction_time_spent_until_committed() -> Panel {
    Panel::new(
        "Transaction Time Spent in Mempool Until Committed",
        "The time a transaction spends in the mempool until it is committed (5m window)",
        HISTOGRAM_QUANTILES
            .iter()
            .map(|q| {
                format!(
                    "histogram_quantile({q:.2}, sum by (le) (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                    TRANSACTION_TIME_SPENT_UNTIL_COMMITTED.get_name_with_filter(),
                )
            })
            .collect(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_mempool_row() -> Row {
    Row::new(
        "Mempool",
        vec![
            get_panel_mempool_transactions_received_rate(),
            get_panel_mempool_transactions_received(),
            get_panel_mempool_transactions_committed(),
            get_panel_mempool_transactions_dropped(),
            get_panel_mempool_pool_size(),
            get_panel_mempool_total_size_in_bytes(),
            get_panel_mempool_priority_queue_size(),
            get_panel_mempool_pending_queue_size(),
            get_panel_mempool_delayed_declares_size(),
            get_panel_mempool_transaction_time_spent_until_committed(),
        ],
    )
}
