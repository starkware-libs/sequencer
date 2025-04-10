use apollo_infra::metrics::{
    MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(MEMPOOL_P2P_NUM_CONNECTED_PEERS, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_SENT_MESSAGES, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_BROADCASTED_BATCH_SIZE: Panel =
    Panel::from_hist(MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, PanelType::Stat);

pub(crate) const PANEL_MEMPOOL_P2P_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_LOCAL_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_P2P_LOCAL_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_MEMPOOL_P2P_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(MEMPOOL_P2P_LOCAL_QUEUE_DEPTH, PanelType::Stat);
