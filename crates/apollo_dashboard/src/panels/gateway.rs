use apollo_gateway::metrics::{
    GATEWAY_ADD_TX_FAILURE,
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_TRANSACTIONS_FAILED,
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS,
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME,
    GATEWAY_VALIDATE_TX_LATENCY,
    LABEL_NAME_ADD_TX_FAILURE_REASON,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE as GATEWAY_LABEL_NAME_TX_TYPE,
};
use apollo_metrics::MetricCommon;

use crate::dashboard::{Panel, PanelType, Row, Unit};
use crate::query_builder::{sum_by_label, DisplayMethod, RANGE_DURATION};

fn get_panel_gateway_transactions_received_by_type() -> Panel {
    Panel::new(
        "Transactions Received by Type",
        "The number of transactions received by type (over the selected time range)",
        sum_by_label(
            &GATEWAY_TRANSACTIONS_RECEIVED,
            GATEWAY_LABEL_NAME_TX_TYPE,
            DisplayMethod::Increase(RANGE_DURATION),
            false,
        ),
        PanelType::Stat,
    )
    .with_log_query("\"Processing tx\"")
}

fn get_panel_gateway_transactions_received_by_source() -> Panel {
    Panel::new(
        "Transactions Received by Source",
        "The number of transactions received by source (over the selected time range)",
        sum_by_label(
            &GATEWAY_TRANSACTIONS_RECEIVED,
            LABEL_NAME_SOURCE,
            DisplayMethod::Increase(RANGE_DURATION),
            false,
        ),
        PanelType::Stat,
    )
    .with_log_query("\"Processing tx\" AND \"is_p2p=\"")
}

fn get_panel_gateway_transactions_received_rate() -> Panel {
    Panel::new(
        "Gateway Transactions Received Rate (TPS)",
        "The rate of transactions received by the gateway (1m window)",
        format!(
            "sum(rate({}[1m])) or vector(0)",
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
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

pub(crate) fn get_panel_gateway_add_tx_failure_by_reason() -> Panel {
    Panel::new(
        "Transactions Failed by Reason",
        "The number of transactions failed by reason (over the selected time range)",
        sum_by_label(
            &GATEWAY_ADD_TX_FAILURE,
            LABEL_NAME_ADD_TX_FAILURE_REASON,
            DisplayMethod::Increase(RANGE_DURATION),
            true,
        ),
        PanelType::Stat,
    )
}

fn get_panel_gateway_transactions_failure_rate() -> Panel {
    let sum_failed = sum_by_label(
        &GATEWAY_TRANSACTIONS_FAILED,
        GATEWAY_LABEL_NAME_TX_TYPE,
        DisplayMethod::Increase(RANGE_DURATION),
        false,
    );
    let sum_received = sum_by_label(
        &GATEWAY_TRANSACTIONS_RECEIVED,
        GATEWAY_LABEL_NAME_TX_TYPE,
        DisplayMethod::Increase(RANGE_DURATION),
        false,
    );
    Panel::new(
        "Transaction Failure Rate by Type",
        "The rate of failed transactions vs received transactions by type (over the selected time \
         range)",
        format!("({sum_failed} / {sum_received})",),
        PanelType::Stat,
    )
    .with_unit(Unit::PercentUnit)
}

fn get_panel_gateway_transactions_sent_to_mempool() -> Panel {
    Panel::new(
        "Transactions Sent to Mempool by Type",
        "The number of transactions sent to mempool by type (over the selected time range)",
        sum_by_label(
            &GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
            GATEWAY_LABEL_NAME_TX_TYPE,
            DisplayMethod::Increase(RANGE_DURATION),
            false,
        ),
        PanelType::Stat,
    )
}

fn get_panel_gateway_validate_stateful_tx_storage_time() -> Panel {
    Panel::from_hist(
        &GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME,
        "Gateway Validate Stateful Tx Storage Access Time",
        "Total time spent in storage operations during stateful tx validation",
    )
    .with_unit(Unit::Seconds)
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
            get_panel_gateway_validate_stateful_tx_storage_time(),
            get_panel_gateway_validate_stateful_tx_storage_operations(),
        ],
    )
}
