use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    LAST_BATCHED_BLOCK,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
    REJECTED_TRANSACTIONS,
    REVERTED_TRANSACTIONS,
};
use apollo_infra::metrics::{
    BATCHER_LOCAL_MSGS_PROCESSED,
    BATCHER_LOCAL_MSGS_RECEIVED,
    BATCHER_LOCAL_QUEUE_DEPTH,
    BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
    BATCHER_REMOTE_MSGS_PROCESSED,
    BATCHER_REMOTE_MSGS_RECEIVED,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_proposal_started() -> Panel {
    Panel::from_counter(PROPOSAL_STARTED, PanelType::Stat)
}
fn get_panel_proposal_succeeded() -> Panel {
    Panel::from_counter(PROPOSAL_SUCCEEDED, PanelType::Stat)
}
fn get_panel_proposal_aborted() -> Panel {
    Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat)
}
fn get_panel_proposal_failed() -> Panel {
    Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat)
}
fn get_panel_batched_transactions() -> Panel {
    Panel::from_counter(BATCHED_TRANSACTIONS, PanelType::Stat)
}
fn get_panel_last_batched_block() -> Panel {
    Panel::from_gauge(LAST_BATCHED_BLOCK, PanelType::Stat)
}

fn get_panel_batcher_local_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_batcher_local_msgs_processed() -> Panel {
    Panel::from_counter(BATCHER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_batcher_remote_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_batcher_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_batcher_remote_msgs_processed() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_batcher_local_queue_depth() -> Panel {
    Panel::from_gauge(BATCHER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_batcher_remote_client_send_attempts() -> Panel {
    Panel::from_hist(BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_panel_rejection_ratio() -> Panel {
    Panel::from_ratio(
        "rejection_ratio",
        "Ratio of rejected transactions out of all processed, over the last 5 minutes",
        &REJECTED_TRANSACTIONS,
        &[&REJECTED_TRANSACTIONS, &BATCHED_TRANSACTIONS],
        "5m",
    )
}
fn get_panel_reverted_transaction_ratio() -> Panel {
    Panel::from_ratio(
        "reverted_transactions_ratio",
        "Ratio of reverted transactions out of all processed, over the last 5 minutes",
        &REVERTED_TRANSACTIONS,
        &[&BATCHED_TRANSACTIONS],
        "5m",
    )
}

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            get_panel_proposal_aborted(),
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

pub(crate) fn get_batcher_infra_row() -> Row {
    Row::new(
        "Batcher Infra",
        vec![
            get_panel_batcher_local_msgs_received(),
            get_panel_batcher_local_msgs_processed(),
            get_panel_batcher_local_queue_depth(),
            get_panel_batcher_remote_msgs_received(),
            get_panel_batcher_remote_valid_msgs_received(),
            get_panel_batcher_remote_msgs_processed(),
            get_panel_batcher_remote_client_send_attempts(),
        ],
    )
}
