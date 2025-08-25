use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    BATCHER_LABELED_PROCESSING_TIMES_SECS,
    BATCHER_LABELED_QUEUEING_TIMES_SECS,
    BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
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
    BATCHER_REMOTE_NUMBER_OF_CONNECTIONS,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{create_request_type_labeled_hist_panels, Panel, PanelType, Row};

fn get_panel_proposal_started() -> Panel {
    Panel::from_counter(PROPOSAL_STARTED, PanelType::Stat)
}
fn get_panel_proposal_succeeded() -> Panel {
    Panel::from_counter(PROPOSAL_SUCCEEDED, PanelType::Stat)
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
fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(BATCHER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(BATCHER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_number_of_connections() -> Panel {
    Panel::from_gauge(BATCHER_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
}
fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(BATCHER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_processing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        BATCHER_LABELED_PROCESSING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_queueing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        BATCHER_LABELED_QUEUEING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_local_client_response_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_remote_client_response_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
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

fn get_panel_batched_transactions_rate() -> Panel {
    Panel::new(
        "batched_transactions_rate (TPS)",
        "The rate of transactions batched by the Batcher during the last minute",
        vec![format!(
            "min(rate({}[1m])) or vector(0)",
            BATCHED_TRANSACTIONS.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_remote_client_communication_failure_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
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
            get_panel_batched_transactions(),
            get_panel_last_batched_block(),
            get_panel_rejection_ratio(),
            get_panel_reverted_transaction_ratio(),
        ],
    )
}

// TODO(Tsabary): this can be a macro used across all infra component panels.
pub(crate) fn get_batcher_infra_row() -> Row {
    Row::new(
        "Batcher Infra",
        vec![
            get_panel_local_msgs_received(),
            get_panel_local_msgs_processed(),
            get_panel_local_queue_depth(),
            get_panel_remote_msgs_received(),
            get_panel_remote_valid_msgs_received(),
            get_panel_remote_msgs_processed(),
            get_panel_remote_number_of_connections(),
            get_panel_remote_client_send_attempts(),
        ]
        .into_iter()
        .chain(get_processing_times_panels())
        .chain(get_queueing_times_panels())
        .chain(get_local_client_response_times_panels())
        .chain(get_remote_client_response_times_panels())
        .chain(get_remote_client_communication_failure_times_panels())
        .collect::<Vec<_>>(),
    )
}
