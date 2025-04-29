use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_BUILD_PROPOSAL_TOTAL,
    CONSENSUS_CACHED_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_HELD_LOCKS,
    CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    CONSENSUS_NEW_VALUE_LOCKS,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_PROPOSALS_RECEIVED,
    CONSENSUS_PROPOSALS_VALIDATED,
    CONSENSUS_PROPOSALS_VALID_INIT,
    CONSENSUS_REPROPOSALS,
    CONSENSUS_ROUND,
    CONSENSUS_TIMEOUTS,
    LABEL_NAME_TIMEOUT_REASON,
};
use apollo_consensus_manager::metrics::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use apollo_consensus_orchestrator::metrics::{
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_BLOB_SUCCESS,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_DATA_GAS_MISMATCH,
    CONSENSUS_L1_GAS_MISMATCH,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
    LABEL_CENDE_FAILURE_REASON,
};
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_CONSENSUS_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CONSENSUS_BLOCK_NUMBER, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_ROUND: Panel = Panel::from_gauge(CONSENSUS_ROUND, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CONSENSUS_MAX_CACHED_BLOCK_NUMBER, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_CACHED_VOTES: Panel =
    Panel::from_gauge(CONSENSUS_CACHED_VOTES, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS: Panel =
    Panel::from_counter(CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC: Panel =
    Panel::from_counter(CONSENSUS_DECISIONS_REACHED_BY_SYNC, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_RECEIVED: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_VALID_INIT: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_VALID_INIT, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_VALIDATED: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_VALIDATED, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_INVALID: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_INVALID, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL: Panel =
    Panel::from_counter(CONSENSUS_BUILD_PROPOSAL_TOTAL, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED: Panel =
    Panel::from_counter(CONSENSUS_BUILD_PROPOSAL_FAILED, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_REPROPOSALS: Panel =
    Panel::from_counter(CONSENSUS_REPROPOSALS, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_NEW_VALUE_LOCKS: Panel =
    Panel::from_counter(CONSENSUS_NEW_VALUE_LOCKS, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_HELD_LOCKS: Panel =
    Panel::from_counter(CONSENSUS_HELD_LOCKS, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_TIMEOUTS_BY_TYPE: Panel = Panel::new(
    CONSENSUS_TIMEOUTS.get_name(),
    CONSENSUS_TIMEOUTS.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        LABEL_NAME_TIMEOUT_REASON,
        CONSENSUS_TIMEOUTS.get_name_with_filter()
    ),
    PanelType::Stat,
);
pub(crate) const PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL: Panel =
    Panel::from_gauge(CONSENSUS_NUM_BATCHES_IN_PROPOSAL, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL: Panel =
    Panel::from_gauge(CONSENSUS_NUM_TXS_IN_PROPOSAL, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_L2_GAS_PRICE: Panel =
    Panel::from_gauge(CONSENSUS_L2_GAS_PRICE, PanelType::Stat);

pub(crate) const PANEL_CONSENSUS_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(CONSENSUS_NUM_CONNECTED_PEERS, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_VOTES_NUM_SENT_MESSAGES, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, PanelType::Stat);

pub(crate) const PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER, PanelType::Stat);
pub(crate) const PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY: Panel = Panel::new(
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_name(),
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_description(),
    formatcp!(
        "avg_over_time({}[2m])",
        CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_name_with_filter()
    ),
    PanelType::Stat,
);
pub(crate) const PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY: Panel = Panel::new(
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name(),
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_description(),
    formatcp!("avg_over_time({}[2m])", CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_with_filter()),
    PanelType::Stat,
);
pub(crate) const PANEL_CENDE_WRITE_BLOB_SUCCESS: Panel =
    Panel::from_counter(CENDE_WRITE_BLOB_SUCCESS, PanelType::Graph);
pub(crate) const PANEL_CENDE_WRITE_BLOB_FAILURE: Panel = Panel::new(
    CENDE_WRITE_BLOB_FAILURE.get_name(),
    CENDE_WRITE_BLOB_FAILURE.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        LABEL_CENDE_FAILURE_REASON,
        CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()
    ),
    PanelType::Stat,
);
pub(crate) const PANEL_CONSENSUS_L1_DATA_GAS_MISMATCH: Panel =
    Panel::from_counter(CONSENSUS_L1_DATA_GAS_MISMATCH, PanelType::Stat);
pub(crate) const PANEL_CONSENSUS_L1_GAS_MISMATCH: Panel =
    Panel::from_counter(CONSENSUS_L1_GAS_MISMATCH, PanelType::Stat);
