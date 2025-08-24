use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    LAST_BATCHED_BLOCK,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    REVERTED_TRANSACTIONS,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_proposal_started() -> Panel {
    Panel::from_counter(&PROPOSAL_STARTED, PanelType::Stat)
}
fn get_panel_proposal_succeeded() -> Panel {
    Panel::from_counter(&PROPOSAL_SUCCEEDED, PanelType::Stat)
}
fn get_panel_proposal_failed() -> Panel {
    Panel::from_counter(&PROPOSAL_FAILED, PanelType::Stat)
}
fn get_panel_batched_transactions() -> Panel {
    Panel::from_counter(&BATCHED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_last_batched_block() -> Panel {
    Panel::from_gauge(&LAST_BATCHED_BLOCK, PanelType::Stat)
}
fn get_panel_rejection_ratio() -> Panel {
    Panel::ratio_time_series(
        "rejection_ratio",
        "Ratio of rejected transactions out of all processed, over the last 5 minutes",
        &REJECTED_TRANSACTIONS,
        &[&REJECTED_TRANSACTIONS, &BATCHED_TRANSACTIONS],
        "5m",
    )
}
fn get_panel_reverted_transaction_ratio() -> Panel {
    Panel::ratio_time_series(
        "reverted_transactions_ratio",
        "Ratio of reverted transactions out of all processed, over the last 5 minutes",
        &REVERTED_TRANSACTIONS,
        &[&REJECTED_TRANSACTIONS, &BATCHED_TRANSACTIONS],
        "5m",
    )
}

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_proposal_started(),
            get_panel_proposal_succeeded(),
            get_panel_proposal_failed(),
            get_panel_batched_transactions(),
            get_panel_last_batched_block(),
            get_panel_rejection_ratio(),
            get_panel_reverted_transaction_ratio(),
        ],
    )
}
