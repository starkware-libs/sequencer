use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    PROPOSAL_ABORTED,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    PROPOSER_DEFERRED_TXS,
    REJECTED_TRANSACTIONS,
    REVERTED_BLOCKS,
    REVERTED_TRANSACTIONS,
    STORAGE_HEIGHT,
    VALIDATOR_WASTED_TXS,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_proposal_started() -> Panel {
    Panel::new(
        "Proposal Started",
        "Number of proposals started (10m window)",
        vec![format!("increase({}[10m])", PROPOSAL_STARTED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_proposal_succeeded() -> Panel {
    Panel::new(
        "Proposal Succeeded",
        "Number of proposals succeeded (10m window)",
        vec![format!("increase({}[10m])", PROPOSAL_SUCCEEDED.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_proposal_failed() -> Panel {
    Panel::new(
        "Proposal Failed / Aborted",
        "Number of proposals failed / aborted (10m window)",
        vec![
            format!("increase({}[10m])", PROPOSAL_FAILED.get_name_with_filter()),
            format!("increase({}[10m])", PROPOSAL_ABORTED.get_name_with_filter()),
        ],
        PanelType::TimeSeries,
    )
    .with_legends(vec!["Failed", "Aborted"])
}
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
fn get_panel_reverted_blocks() -> Panel {
    Panel::new(
        "Reverted Blocks",
        "Number of blocks reverted (10m window)",
        vec![format!("increase({}[10m])", REVERTED_BLOCKS.get_name_with_filter())],
        PanelType::TimeSeries,
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

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_batched_transactions_rate(),
            get_panel_proposal_started(),
            get_panel_proposal_succeeded(),
            get_panel_proposal_failed(),
            get_panel_validator_wasted_txs(),
            get_panel_proposer_deferred_txs(),
            get_panel_storage_height(),
            get_panel_rejection_reverted_ratio(),
            get_panel_reverted_blocks(),
        ],
    )
}
