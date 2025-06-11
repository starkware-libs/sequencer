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
    BATCHER_REMOTE_MSGS_PROCESSED,
    BATCHER_REMOTE_MSGS_RECEIVED,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_PROPOSAL_STARTED: Panel =
    Panel::from_counter(PROPOSAL_STARTED, PanelType::Stat);
pub(crate) const PANEL_PROPOSAL_SUCCEEDED: Panel =
    Panel::from_counter(PROPOSAL_SUCCEEDED, PanelType::Stat);
pub(crate) const PANEL_PROPOSAL_ABORTED: Panel =
    Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat);
pub(crate) const PANEL_PROPOSAL_FAILED: Panel =
    Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat);
pub(crate) const PANEL_BATCHED_TRANSACTIONS: Panel =
    Panel::from_counter(BATCHED_TRANSACTIONS, PanelType::Stat);
pub(crate) const PANEL_LAST_BATCHED_BLOCK: Panel = Panel::new(
    "Last batched block",
    "The last block number that was batched",
    LAST_BATCHED_BLOCK.get_name_with_filter(),
    PanelType::Stat,
);

pub(crate) const PANEL_BATCHER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_BATCHER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_BATCHER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_BATCHER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_BATCHER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(BATCHER_LOCAL_QUEUE_DEPTH, PanelType::Graph);
