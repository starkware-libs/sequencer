use apollo_infra::metrics::{
    STATE_SYNC_LOCAL_MSGS_PROCESSED,
    STATE_SYNC_LOCAL_MSGS_RECEIVED,
    STATE_SYNC_LOCAL_QUEUE_DEPTH,
    STATE_SYNC_REMOTE_MSGS_PROCESSED,
    STATE_SYNC_REMOTE_MSGS_RECEIVED,
    STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_state_sync_metrics::metrics::{
    P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    P2P_SYNC_NUM_CONNECTED_PEERS,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_REVERTED_TRANSACTIONS,
};

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_STATE_SYNC_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(STATE_SYNC_LOCAL_QUEUE_DEPTH, PanelType::Stat);

pub(crate) const PANEL_P2P_SYNC_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_CONNECTED_PEERS, PanelType::Stat);
pub(crate) const PANEL_P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, PanelType::Stat);
pub(crate) const PANEL_P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_PROCESSED_TRANSACTIONS: Panel =
    Panel::from_counter(STATE_SYNC_PROCESSED_TRANSACTIONS, PanelType::Stat);
pub(crate) const PANEL_STATE_SYNC_REVERTED_TRANSACTIONS: Panel =
    Panel::from_counter(STATE_SYNC_REVERTED_TRANSACTIONS, PanelType::Stat);
