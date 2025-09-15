use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    PROPOSER_DEFERRED_TXS,
    REJECTED_TRANSACTIONS,
    REVERTED_TRANSACTIONS,
    STORAGE_HEIGHT,
    VALIDATOR_WASTED_TXS,
};
use apollo_consensus_orchestrator::metrics::{
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_validator_wasted_txs() -> Panel {
    Panel::new(
        "Proposal Validation: Wasted TXs",
        "Number of txs executed by the validator but excluded from the block (10m window)",
        vec![format!("increase({}[10m])", VALIDATOR_WASTED_TXS.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_proposer_deferred_txs() -> Panel {
    Panel::new(
        "Proposal Build: Deferred TXs",
        "Number of txs started execution by the proposer but excluded from the block (10m window)",
        vec![format!("increase({}[10m])", PROPOSER_DEFERRED_TXS.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_storage_height() -> Panel {
    Panel::new(
        "Storage Height",
        "The height of the batcher's storage",
        vec![STORAGE_HEIGHT.get_name_with_filter().to_string()],
        PanelType::Stat,
    )
}
fn get_panel_rejection_reverted_ratio() -> Panel {
    let denominator_expr = format!(
        "(increase({}[10m]) + increase({}[10m]) + increase({}[10m]))",
        REJECTED_TRANSACTIONS.get_name_with_filter(),
        REVERTED_TRANSACTIONS.get_name_with_filter(),
        BATCHED_TRANSACTIONS.get_name_with_filter(),
    );
    Panel::new(
        "Rejected / Reverted TXs Ratio",
        "Ratio of rejected / reverted transactions out of all processed txs (10m window)",
        vec![
            format!(
                "increase({}[10m]) / {denominator_expr}",
                REJECTED_TRANSACTIONS.get_name_with_filter(),
            ),
            format!(
                "increase({}[10m]) / {denominator_expr}",
                REVERTED_TRANSACTIONS.get_name_with_filter(),
            ),
        ],
        PanelType::TimeSeries,
    )
    .with_legends(vec!["Rejected", "Reverted"])
    .with_unit(Unit::PercentUnit)
}
fn get_panel_batched_transactions_rate() -> Panel {
    Panel::new(
        "Batched Transactions Rate (TPS)",
        "The rate of transactions batched by the Batcher (1m window)",
        vec![format!("rate({}[1m])", BATCHED_TRANSACTIONS.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_num_batches_in_proposal() -> Panel {
    Panel::new(
        "Proposal Validation: Number of Batches in Proposal",
        "The number of transaction batches received in a valid proposal",
        vec![CONSENSUS_NUM_BATCHES_IN_PROPOSAL.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}
fn get_panel_num_txs_in_proposal() -> Panel {
    Panel::new(
        "Proposal Validation: Number of Transactions in Proposal",
        "The total number of individual transactions in a valid proposal received",
        vec![CONSENSUS_NUM_TXS_IN_PROPOSAL.get_name_with_filter().to_string()],
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_storage_height(),
            get_panel_batched_transactions_rate(),
            get_panel_proposer_deferred_txs(),
            get_panel_validator_wasted_txs(),
            get_panel_rejection_reverted_ratio(),
            get_panel_num_batches_in_proposal(),
            get_panel_num_txs_in_proposal(),
        ],
    )
}
