use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    BLOCK_CLOSE_REASON,
    BUILDING_HEIGHT,
    COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
    COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY,
    COMMITMENT_MANAGER_NUM_COMMIT_RESULTS,
    COMMITMENT_MANAGER_REVERT_BLOCK_COUNT,
    COMMITMENT_MANAGER_REVERT_BLOCK_LATENCY,
    EVENT_COMMITMENT_COUNT,
    EVENT_COMMITMENT_LATENCY,
    GLOBAL_ROOT_HEIGHT,
    LABEL_NAME_BLOCK_CLOSE_REASON,
    PROPOSER_DEFERRED_TXS,
    RECEIPT_COMMITMENT_LATENCY,
    REJECTED_TRANSACTIONS,
    REVERTED_TRANSACTIONS,
    STATE_DIFF_COMMITMENT_LATENCY,
    STATE_DIFF_LENGTH,
    TX_COMMITMENT_COUNT,
    TX_COMMITMENT_LATENCY,
    VALIDATOR_WASTED_TXS,
};
use apollo_consensus::metrics::CONSENSUS_BLOCK_NUMBER;
use apollo_consensus_orchestrator::metrics::{
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType, Unit};
use crate::query_builder::{increase, sum_by_label, DisplayMethod, DEFAULT_DURATION};

const COMMITMENT_LATENCIES_LOG_QUERY: &str = "\"Block\" AND \"commitment latencies\"";

/// Returns a panel that shows the average latency (in seconds) over a 1m window:
/// (increase(total_ms)[1m] / 1000) / clamp_min(increase(count)[1m], 1).
fn average_latency_panel(
    name: impl ToString,
    description: impl ToString,
    numerator: &dyn MetricQueryName,
    count: &dyn MetricQueryName,
    divisor: Option<u64>,
    log_query: &str,
    unit: Option<Unit>,
) -> Panel {
    let numerator = format!("({})", increase(numerator, "1m"));
    let count = format!("clamp_min({}, 1)", increase(count, "1m"));
    let divisor = match divisor {
        Some(n) => format!("{} * {}", n, count),
        None => count,
    };
    let expr = format!("{} / {}", numerator, divisor);
    let mut panel = Panel::new(name, description, expr, PanelType::TimeSeries);
    if let Some(u) = unit {
        panel = panel.with_unit(u);
    }
    panel.with_log_query(log_query)
}

pub(crate) fn get_panel_consensus_block_time_avg() -> Panel {
    Panel::new(
        "Average Block Time",
        "Average block time (1m window)",
        format!("1 / rate({}[1m])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_validator_wasted_txs() -> Panel {
    Panel::new(
        "Proposal Validation: Wasted TXs",
        format!(
            "Number of txs executed by the validator but excluded from the block \
             ({DEFAULT_DURATION} window)",
        ),
        increase(&VALIDATOR_WASTED_TXS, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("Finished building block as validator. Started executing")
}

fn get_panel_proposer_deferred_txs() -> Panel {
    Panel::new(
        "Proposal Build: Deferred TXs",
        format!(
            "Number of txs started execution by the proposer but excluded from the block \
             ({DEFAULT_DURATION} window)",
        ),
        increase(&PROPOSER_DEFERRED_TXS, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("Finished building block as proposer. Started executing")
}

fn get_panel_building_height() -> Panel {
    Panel::new(
        "Building Height",
        "The height of the block that should be built next.",
        BUILDING_HEIGHT.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query("Building block at height")
}

fn get_panel_global_root_height() -> Panel {
    Panel::new(
        "Global Root Height",
        "The height of the first block without global root stored.",
        GLOBAL_ROOT_HEIGHT.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query("Committing block at height")
}

fn get_panel_rejection_reverted_ratio() -> Panel {
    let rejected_txs_expr = increase(&REJECTED_TRANSACTIONS, DEFAULT_DURATION);
    let reverted_txs_expr = increase(&REVERTED_TRANSACTIONS, DEFAULT_DURATION);
    let batched_txs_expr = increase(&BATCHED_TRANSACTIONS, DEFAULT_DURATION);

    let denominator_expr =
        format!("({} + {} + {})", rejected_txs_expr, reverted_txs_expr, batched_txs_expr,);
    Panel::new(
        "Rate of Rejected and Reverted TXs Ratio",
        format!(
            "Rates of the rejected and reverted transactions ratios of all processed txs \
             ({DEFAULT_DURATION} window)"
        ),
        vec![
            format!("{rejected_txs_expr} / {denominator_expr}"),
            format!("{reverted_txs_expr} / {denominator_expr}"),
        ],
        PanelType::TimeSeries,
    )
    .with_legends(vec!["Rejected", "Reverted"])
    .with_unit(Unit::PercentUnit)
}

pub(crate) fn get_panel_batched_transactions_rate() -> Panel {
    Panel::new(
        "Batched Transactions Rate (TPS)",
        "The rate of transactions batched by the Batcher (1m window)",
        format!("rate({}[1m])", BATCHED_TRANSACTIONS.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_log_query("BATCHER_FIN_VALIDATOR")
}

fn get_panel_block_close_reasons() -> Panel {
    Panel::new(
        "Block Close Reasons",
        format!("Number of blocks closed by reason ({} window)", DEFAULT_DURATION),
        sum_by_label(
            &BLOCK_CLOSE_REASON,
            LABEL_NAME_BLOCK_CLOSE_REASON,
            DisplayMethod::Increase(DEFAULT_DURATION),
            false,
        ),
        PanelType::Stat,
    )
    .with_log_query(
        "\"Block builder deadline reached.\" OR \"Block is full.\" OR \"No transactions are being \
         executed and\"",
    )
}

fn get_panel_num_batches_in_proposal() -> Panel {
    Panel::new(
        "Number of Chunks in Proposal",
        "The number of transaction batches received in a valid proposal",
        CONSENSUS_NUM_BATCHES_IN_PROPOSAL.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
}

fn get_panel_num_txs_in_proposal() -> Panel {
    Panel::new(
        "Number of Transactions in Proposal",
        "The total number of individual transactions in a valid proposal received",
        CONSENSUS_NUM_TXS_IN_PROPOSAL.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_log_query("BATCHER_FIN_PROPOSER")
}

fn get_panel_commitment_manager_commit_block_latency() -> Panel {
    average_latency_panel(
        "Commit Block Latency",
        "Average latency of commit tasks in the commitment manager over a 1m window",
        &COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY,
        &COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
        Some(1000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_commitment_manager_revert_block_latency() -> Panel {
    average_latency_panel(
        "Revert Block Latency",
        "Average latency of revert tasks in the commitment manager over a 1m window",
        &COMMITMENT_MANAGER_REVERT_BLOCK_LATENCY,
        &COMMITMENT_MANAGER_REVERT_BLOCK_COUNT,
        Some(1000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_commitment_manager_results_count() -> Panel {
    Panel::new(
        "Commitment Manager Results Count",
        "The number of commit results received from the commitment manager (1m window)",
        increase(&COMMITMENT_MANAGER_NUM_COMMIT_RESULTS, "1m"),
        PanelType::TimeSeries,
    )
}

fn get_panel_txs_commitment_latency() -> Panel {
    average_latency_panel(
        "Transaction Commitment Latency",
        "Average duration of transaction commitment computation over a 1m window",
        &TX_COMMITMENT_LATENCY,
        &COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_txs_commitment_per_tx_latency() -> Panel {
    average_latency_panel(
        "Transaction Commitment Per Transaction Latency",
        "Average duration of transaction commitment per transaction over a 1m window",
        &TX_COMMITMENT_LATENCY,
        &TX_COMMITMENT_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_events_commitment_latency() -> Panel {
    average_latency_panel(
        "Event Commitment Latency",
        "Average duration of event commitment computation over a 1m window",
        &EVENT_COMMITMENT_LATENCY,
        &COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_events_commitment_per_event_latency() -> Panel {
    average_latency_panel(
        "Event Commitment Per Event Latency",
        "Average duration of event commitment per event over a 1m window",
        &EVENT_COMMITMENT_LATENCY,
        &EVENT_COMMITMENT_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_receipts_commitment_latency() -> Panel {
    average_latency_panel(
        "Receipt Commitment Latency",
        "Average duration of receipt commitment computation over a 1m window",
        &RECEIPT_COMMITMENT_LATENCY,
        &COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_state_diff_commitment_latency() -> Panel {
    average_latency_panel(
        "State Diff Commitment Latency",
        "Average duration of state diff commitment computation over a 1m window",
        &STATE_DIFF_COMMITMENT_LATENCY,
        &COMMITMENT_MANAGER_COMMIT_BLOCK_COUNT,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

fn get_panel_state_diff_commitment_per_state_diff_length() -> Panel {
    average_latency_panel(
        "State Diff Commitment Per State Diff Length",
        "Average duration of state diff commitment per state diff length over a 1m window",
        &STATE_DIFF_COMMITMENT_LATENCY,
        &STATE_DIFF_LENGTH,
        Some(1_000_000),
        COMMITMENT_LATENCIES_LOG_QUERY,
        Some(Unit::Seconds),
    )
}

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_building_height(),
            get_panel_global_root_height(),
            get_panel_consensus_block_time_avg(),
            get_panel_batched_transactions_rate(),
            get_panel_proposer_deferred_txs(),
            get_panel_validator_wasted_txs(),
            get_panel_rejection_reverted_ratio(),
            get_panel_block_close_reasons(),
            get_panel_num_batches_in_proposal(),
            get_panel_num_txs_in_proposal(),
            get_panel_commitment_manager_commit_block_latency(),
            get_panel_commitment_manager_results_count(),
            get_panel_commitment_manager_revert_block_latency(),
            get_panel_txs_commitment_latency(),
            get_panel_txs_commitment_per_tx_latency(),
            get_panel_events_commitment_latency(),
            get_panel_events_commitment_per_event_latency(),
            get_panel_receipts_commitment_latency(),
            get_panel_state_diff_commitment_latency(),
            get_panel_state_diff_commitment_per_state_diff_length(),
        ],
    )
}
