use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    BLOCK_CLOSE_REASON,
    LABEL_NAME_BLOCK_CLOSE_REASON,
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
use apollo_metrics::MetricCommon;

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_validator_wasted_txs() -> Panel {
    Panel::new(
        "Proposal Validation: Wasted TXs",
        "Number of txs executed by the validator but excluded from the block (10m window)",
        format!("increase({}[10m])", VALIDATOR_WASTED_TXS.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_log_query("Finished building block as validator. Started executing")
}

fn get_panel_proposer_deferred_txs() -> Panel {
    Panel::new(
        "Proposal Build: Deferred TXs",
        "Number of txs started execution by the proposer but excluded from the block (10m window)",
        format!("increase({}[10m])", PROPOSER_DEFERRED_TXS.get_name_with_filter()),
        PanelType::TimeSeries,
    )
    .with_log_query("Finished building block as proposer. Started executing")
}

fn get_panel_storage_height() -> Panel {
    Panel::new(
        "Storage Height",
        "The height of the batcher's storage",
        STORAGE_HEIGHT.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
    .with_log_query("Committing block at height")
}

fn get_panel_rejection_reverted_ratio() -> Panel {
    let denominator_expr = format!(
        "(increase({}[10m]) + increase({}[10m]) + increase({}[10m]))",
        REJECTED_TRANSACTIONS.get_name_with_filter(),
        REVERTED_TRANSACTIONS.get_name_with_filter(),
        BATCHED_TRANSACTIONS.get_name_with_filter(),
    );
    Panel::new_multi_expr(
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
        "Number of blocks closed by reason (10m window)",
        format!(
            "sum by ({}) (increase({}[10m]))",
            LABEL_NAME_BLOCK_CLOSE_REASON,
            BLOCK_CLOSE_REASON.get_name_with_filter()
        ),
        PanelType::Stat,
    )
    .with_log_query("\"Block builder deadline reached.\" OR \"Block is full.\"")
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

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_storage_height(),
            get_panel_batched_transactions_rate(),
            get_panel_proposer_deferred_txs(),
            get_panel_validator_wasted_txs(),
            get_panel_rejection_reverted_ratio(),
            get_panel_block_close_reasons(),
            get_panel_num_batches_in_proposal(),
            get_panel_num_txs_in_proposal(),
        ],
    )
}
