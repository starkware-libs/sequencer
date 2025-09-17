use apollo_gateway::metrics::{
    GATEWAY_ADD_TX_FAILURE,
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_TRANSACTIONS_FAILED,
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_MICROS,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS,
    GATEWAY_VALIDATE_TX_LATENCY,
    LABEL_NAME_ADD_TX_FAILURE_REASON,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE as GATEWAY_LABEL_NAME_TX_TYPE,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_gateway_transactions_received_by_type() -> Panel {
    Panel::new(
        "Transactions Received by Type",
        "The number of transactions received by type (over the selected time range)",
        vec![format!(
            "sum  by ({}) (increase({}[$__range])) ",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
    .with_log_query("\"Processing tx\"")
}

fn get_panel_gateway_transactions_received_by_source() -> Panel {
    Panel::new(
        "Transactions Received by Source",
        "The number of transactions received by source (over the selected time range)",
        vec![format!(
            "sum  by ({}) (increase({}[$__range])) ",
            LABEL_NAME_SOURCE,
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
    .with_log_query("\"Processing tx\" AND \"is_p2p=\"")
}

fn get_panel_gateway_transactions_received_rate() -> Panel {
    Panel::new(
        "Gateway Transactions Received Rate (TPS)",
        "The rate of transactions received by the gateway (1m window)",
        vec![format!(
            "sum(rate({}[1m])) or vector(0)",
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_gateway_add_tx_latency() -> Panel {
    Panel::from_hist(
        &GATEWAY_ADD_TX_LATENCY,
        "Add Tx Latency",
        "The time it takes the gateway to add a transaction to the mempool",
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_gateway_validate_tx_latency() -> Panel {
    Panel::from_hist(
        &GATEWAY_VALIDATE_TX_LATENCY,
        "Validate Tx Latency",
        "The time it takes to validate a transaction",
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_gateway_add_tx_failure_by_reason() -> Panel {
    Panel::new(
        "Transactions Failed by Reason",
        "The number of transactions failed by reason (over the selected time range)",
        vec![format!(
            "sum by ({}) (increase({}[$__range])) > 0",
            LABEL_NAME_ADD_TX_FAILURE_REASON,
            GATEWAY_ADD_TX_FAILURE.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

pub(crate) fn get_panel_gateway_transactions_failure_rate() -> Panel {
    Panel::new(
        "Transaction Failure Rate by Type",
        "The rate of failed transactions vs received transactions by type (over the selected time \
         range)",
        vec![format!(
            "(sum by ({}) (increase({}[$__range])) / sum by ({}) (increase({}[$__range])))",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_FAILED.get_name_with_filter(),
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        )],
        PanelType::Stat,
    )
    .with_unit(Unit::PercentUnit)
}

fn get_panel_gateway_transactions_sent_to_mempool() -> Panel {
    Panel::new(
        "Transactions Sent to Mempool by Type",
        "The number of transactions sent to mempool by type (over the selected time range)",
        vec![format!(
            "sum  by ({}) (increase({}[$__range]))",
            GATEWAY_LABEL_NAME_TX_TYPE,
            GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}

fn get_panel_gateway_validate_stateful_tx_storage_micros() -> Panel {
    Panel::from_hist(
        &GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_MICROS,
        "Gateway Validate Stateful Tx Storage Micros",
        "Total time spent in storage operations in micros during stateful tx validation",
    )
}

fn get_panel_gateway_validate_stateful_tx_storage_operations() -> Panel {
    Panel::from_hist(
        &GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS,
        "Gateway Validate Stateful Tx Storage Operations",
        "Total number of storage operations during stateful tx validation",
    )
}

pub(crate) fn get_gateway_row() -> Row {
    Row::new(
        "Gateway",
        vec![
            get_panel_gateway_transactions_received_rate(),
            get_panel_gateway_add_tx_latency(),
            get_panel_gateway_validate_tx_latency(),
            get_panel_gateway_transactions_received_by_source(),
            get_panel_gateway_transactions_received_by_type(),
            get_panel_gateway_transactions_failure_rate(),
            get_panel_gateway_add_tx_failure_by_reason(),
            get_panel_gateway_transactions_sent_to_mempool(),
            get_panel_gateway_validate_stateful_tx_storage_micros(),
            get_panel_gateway_validate_stateful_tx_storage_operations(),
        ],
    )
}
