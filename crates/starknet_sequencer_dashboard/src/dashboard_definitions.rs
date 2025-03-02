use const_format::formatcp;
use starknet_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
};
use starknet_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use starknet_mempool_p2p::metrics::{
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};
use starknet_sequencer_metrics::metric_definitions::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_NUM_SENT_MESSAGES,
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
};

use crate::dashboard::{Dashboard, Panel, PanelType, Row};
#[cfg(test)]
#[path = "dashboard_definitions_test.rs"]
mod dashboard_definitions_test;

pub const DEV_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana.json";

const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel = Panel::new(
    ADDED_TRANSACTIONS_TOTAL.get_name(),
    ADDED_TRANSACTIONS_TOTAL.get_description(),
    ADDED_TRANSACTIONS_TOTAL.get_name(),
    PanelType::Stat,
);

const PANEL_PROPOSAL_STARTED: Panel = Panel::new(
    PROPOSAL_STARTED.get_name(),
    PROPOSAL_STARTED.get_description(),
    PROPOSAL_STARTED.get_name(),
    PanelType::Stat,
);
const PANEL_PROPOSAL_SUCCEEDED: Panel = Panel::new(
    PROPOSAL_SUCCEEDED.get_name(),
    PROPOSAL_SUCCEEDED.get_description(),
    PROPOSAL_SUCCEEDED.get_name(),
    PanelType::Stat,
);
const PANEL_PROPOSAL_FAILED: Panel = Panel::new(
    PROPOSAL_FAILED.get_name(),
    PROPOSAL_FAILED.get_description(),
    PROPOSAL_FAILED.get_name(),
    PanelType::Stat,
);
const PANEL_BATCHED_TRANSACTIONS: Panel = Panel::new(
    BATCHED_TRANSACTIONS.get_name(),
    BATCHED_TRANSACTIONS.get_description(),
    BATCHED_TRANSACTIONS.get_name(),
    PanelType::Stat,
);

const PANEL_CAIRO_NATIVE_CACHE_MISS_RATIO: Panel = Panel::new(
    "cairo_native_cache_miss_ratio",
    "The ratio of cache misses in the Cairo native cache",
    formatcp!(
        "100 * ({} / clamp_min(({} + {}), 1))",
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_HITS.get_name()
    ),
    PanelType::Graph,
);

const PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS: Panel = Panel::new(
    MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name(),
    MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_description(),
    MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name(),
    PanelType::Stat,
);

const PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES: Panel = Panel::new(
    MEMPOOL_P2P_NUM_SENT_MESSAGES.get_name(),
    MEMPOOL_P2P_NUM_SENT_MESSAGES.get_description(),
    MEMPOOL_P2P_NUM_SENT_MESSAGES.get_name(),
    PanelType::Stat,
);

const PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES: Panel = Panel::new(
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES.get_name(),
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES.get_description(),
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES.get_name(),
    PanelType::Stat,
);

const PANEL_CONSENSUS_NUM_CONNECTED_PEERS: Panel = Panel::new(
    CONSENSUS_NUM_CONNECTED_PEERS.get_name(),
    CONSENSUS_NUM_CONNECTED_PEERS.get_description(),
    CONSENSUS_NUM_CONNECTED_PEERS.get_name(),
    PanelType::Stat,
);

const PANEL_CONSENSUS_NUM_SENT_MESSAGES: Panel = Panel::new(
    CONSENSUS_NUM_SENT_MESSAGES.get_name(),
    CONSENSUS_NUM_SENT_MESSAGES.get_description(),
    CONSENSUS_NUM_SENT_MESSAGES.get_name(),
    PanelType::Stat,
);

const PANEL_CONSENSUS_NUM_RECEIVED_MESSAGES: Panel = Panel::new(
    CONSENSUS_NUM_RECEIVED_MESSAGES.get_name(),
    CONSENSUS_NUM_RECEIVED_MESSAGES.get_description(),
    CONSENSUS_NUM_RECEIVED_MESSAGES.get_name(),
    PanelType::Stat,
);

const PANEL_STATE_SYNC_P2P_NUM_CONNECTED_PEERS: Panel = Panel::new(
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS.get_name(),
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS.get_description(),
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS.get_name(),
    PanelType::Stat,
);

const PANEL_STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS: Panel = Panel::new(
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS.get_name(),
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS.get_description(),
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS.get_name(),
    PanelType::Stat,
);

const PANEL_STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS: Panel = Panel::new(
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS.get_name(),
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS.get_description(),
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS.get_name(),
    PanelType::Stat,
);

const MEMPOOL_P2P_ROW: Row<'_> = Row::new(
    "MempoolP2p",
    "Mempool peer to peer metrics",
    &[
        PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS,
        PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES,
        PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    ],
);

const CONSENSUS_P2P_ROW: Row<'_> = Row::new(
    "ConsensusP2p",
    "Consensus peer to peer metrics",
    &[
        PANEL_CONSENSUS_NUM_CONNECTED_PEERS,
        PANEL_CONSENSUS_NUM_SENT_MESSAGES,
        PANEL_CONSENSUS_NUM_RECEIVED_MESSAGES,
    ],
);

const STATE_SYNC_P2P_ROW: Row<'_> = Row::new(
    "StateSyncP2p",
    "State sync peer to peer metrics",
    &[
        PANEL_STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
        PANEL_STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
        PANEL_STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
    ],
);

const BATCHER_ROW: Row<'_> = Row::new(
    "Batcher",
    "Batcher metrics including proposals and transactions",
    &[
        PANEL_PROPOSAL_STARTED,
        PANEL_PROPOSAL_SUCCEEDED,
        PANEL_PROPOSAL_FAILED,
        PANEL_BATCHED_TRANSACTIONS,
        PANEL_CAIRO_NATIVE_CACHE_MISS_RATIO,
    ],
);
const HTTP_SERVER_ROW: Row<'_> = Row::new(
    "Http Server",
    "Http Server metrics including added transactions",
    &[PANEL_ADDED_TRANSACTIONS_TOTAL],
);

pub const SEQUENCER_DASHBOARD: Dashboard<'_> = Dashboard::new(
    "Sequencer Node Dashboard",
    "Monitoring of the decentralized sequencer node",
    &[BATCHER_ROW, HTTP_SERVER_ROW, MEMPOOL_P2P_ROW, CONSENSUS_P2P_ROW, STATE_SYNC_P2P_ROW],
);
