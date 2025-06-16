use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    LAST_BATCHED_BLOCK,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
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

const PANEL_PROPOSAL_STARTED: Panel = Panel::from_counter(PROPOSAL_STARTED, PanelType::Stat);
const PANEL_PROPOSAL_SUCCEEDED: Panel = Panel::from_counter(PROPOSAL_SUCCEEDED, PanelType::Stat);
const PANEL_PROPOSAL_ABORTED: Panel = Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat);
const PANEL_PROPOSAL_FAILED: Panel = Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat);
const PANEL_BATCHED_TRANSACTIONS: Panel =
    Panel::from_counter(BATCHED_TRANSACTIONS, PanelType::Stat);
const PANEL_LAST_BATCHED_BLOCK: Panel = Panel::from_gauge(LAST_BATCHED_BLOCK, PanelType::Stat);

const PANEL_BATCHER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_RECEIVED, PanelType::Graph);
const PANEL_BATCHER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_PROCESSED, PanelType::Graph);
const PANEL_BATCHER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_RECEIVED, PanelType::Graph);
const PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Graph);
const PANEL_BATCHER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_PROCESSED, PanelType::Graph);
const PANEL_BATCHER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(BATCHER_LOCAL_QUEUE_DEPTH, PanelType::Graph);
const PANEL_BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS: Panel =
    Panel::from_hist(BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::Graph);

pub(crate) fn get_batcher_row() -> Row {
    Row::new(
        "Batcher",
        vec![
            PANEL_PROPOSAL_ABORTED,
            PANEL_PROPOSAL_STARTED,
            PANEL_PROPOSAL_SUCCEEDED,
            PANEL_PROPOSAL_FAILED,
            PANEL_BATCHED_TRANSACTIONS,
            PANEL_LAST_BATCHED_BLOCK,
        ],
    )
}

pub(crate) fn get_batcher_infra_row() -> Row {
    Row::new(
        "Batcher Infra",
        vec![
            PANEL_BATCHER_LOCAL_MSGS_RECEIVED,
            PANEL_BATCHER_LOCAL_MSGS_PROCESSED,
            PANEL_BATCHER_LOCAL_QUEUE_DEPTH,
            PANEL_BATCHER_REMOTE_MSGS_RECEIVED,
            PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_BATCHER_REMOTE_MSGS_PROCESSED,
            PANEL_BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}
