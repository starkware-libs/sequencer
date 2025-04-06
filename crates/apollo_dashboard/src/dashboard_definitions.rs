use apollo_batcher::metrics::{
    BATCHED_TRANSACTIONS,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
};
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
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};
use apollo_gateway::metrics::{
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_VALIDATE_TX_LATENCY,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE as GATEWAY_LABEL_NAME_TX_TYPE,
    TRANSACTIONS_FAILED,
    TRANSACTIONS_RECEIVED,
    TRANSACTIONS_SENT_TO_MEMPOOL,
};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_l1_gas_price::metrics::L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY;
use apollo_mempool::metrics::{
    LABEL_NAME_DROP_REASON,
    LABEL_NAME_TX_TYPE as MEMPOOL_LABEL_NAME_TX_TYPE,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_TOTAL_SIZE_BYTES,
    MEMPOOL_TRANSACTIONS_COMMITTED,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
    TRANSACTION_TIME_SPENT_IN_MEMPOOL,
};
use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};
use apollo_state_reader::metrics::{CLASS_CACHE_HITS, CLASS_CACHE_MISSES, NATIVE_CLASS_RETURNED};
use apollo_state_sync::metrics::{
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
};
use const_format::formatcp;

use crate::dashboard::{Dashboard, Panel, PanelType, Row};

#[cfg(test)]
#[path = "dashboard_definitions_test.rs"]
mod dashboard_definitions_test;

pub const DEV_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana.json";

const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel =
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::Stat);
const PANEL_PROPOSAL_STARTED: Panel = Panel::from_counter(PROPOSAL_STARTED, PanelType::Stat);
const PANEL_PROPOSAL_SUCCEEDED: Panel = Panel::from_counter(PROPOSAL_SUCCEEDED, PanelType::Stat);
const PANEL_PROPOSAL_FAILED: Panel = Panel::from_counter(PROPOSAL_FAILED, PanelType::Stat);
const PANEL_BATCHED_TRANSACTIONS: Panel =
    Panel::from_counter(BATCHED_TRANSACTIONS, PanelType::Stat);

const PANEL_CONSENSUS_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CONSENSUS_BLOCK_NUMBER, PanelType::Stat);
const PANEL_CONSENSUS_ROUND: Panel = Panel::from_gauge(CONSENSUS_ROUND, PanelType::Stat);
const PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CONSENSUS_MAX_CACHED_BLOCK_NUMBER, PanelType::Stat);
const PANEL_CONSENSUS_CACHED_VOTES: Panel =
    Panel::from_gauge(CONSENSUS_CACHED_VOTES, PanelType::Stat);
const PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS: Panel =
    Panel::from_counter(CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS, PanelType::Stat);
const PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC: Panel =
    Panel::from_counter(CONSENSUS_DECISIONS_REACHED_BY_SYNC, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_RECEIVED: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_RECEIVED, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_VALID_INIT: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_VALID_INIT, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_VALIDATED: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_VALIDATED, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_INVALID: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_INVALID, PanelType::Stat);
const PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL: Panel =
    Panel::from_counter(CONSENSUS_BUILD_PROPOSAL_TOTAL, PanelType::Stat);
const PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED: Panel =
    Panel::from_counter(CONSENSUS_BUILD_PROPOSAL_FAILED, PanelType::Stat);
const PANEL_CONSENSUS_REPROPOSALS: Panel =
    Panel::from_counter(CONSENSUS_REPROPOSALS, PanelType::Stat);
const PANEL_CONSENSUS_NEW_VALUE_LOCKS: Panel =
    Panel::from_counter(CONSENSUS_NEW_VALUE_LOCKS, PanelType::Stat);
const PANEL_CONSENSUS_HELD_LOCKS: Panel =
    Panel::from_counter(CONSENSUS_HELD_LOCKS, PanelType::Stat);
const PANEL_CONSENSUS_TIMEOUTS_BY_TYPE: Panel = Panel::new(
    CONSENSUS_TIMEOUTS.get_name(),
    CONSENSUS_TIMEOUTS.get_description(),
    formatcp!("sum  by ({}) ({})", LABEL_NAME_TIMEOUT_REASON, CONSENSUS_TIMEOUTS.get_name()),
    PanelType::Stat,
);
const PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL: Panel =
    Panel::from_gauge(CONSENSUS_NUM_BATCHES_IN_PROPOSAL, PanelType::Stat);
const PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL: Panel =
    Panel::from_gauge(CONSENSUS_NUM_TXS_IN_PROPOSAL, PanelType::Stat);
const PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(MEMPOOL_P2P_NUM_CONNECTED_PEERS, PanelType::Stat);
const PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_SENT_MESSAGES, PanelType::Stat);
const PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, PanelType::Stat);
const PANEL_MEMPOOL_P2P_BROADCASTED_BATCH_SIZE: Panel =
    Panel::from_hist(MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, PanelType::Stat);
const PANEL_CONSENSUS_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(CONSENSUS_NUM_CONNECTED_PEERS, PanelType::Stat);
const PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_VOTES_NUM_SENT_MESSAGES, PanelType::Stat);
const PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES, PanelType::Stat);
const PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES, PanelType::Stat);
const PANEL_STATE_SYNC_P2P_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(STATE_SYNC_P2P_NUM_CONNECTED_PEERS, PanelType::Stat);
const PANEL_STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS: Panel =
    Panel::from_gauge(STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS, PanelType::Stat);
const PANEL_STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS: Panel =
    Panel::from_gauge(STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS, PanelType::Stat);
const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_TYPE: Panel = Panel::new(
    TRANSACTIONS_RECEIVED.get_name(),
    TRANSACTIONS_RECEIVED.get_description(),
    formatcp!("sum  by ({}) ({}) ", GATEWAY_LABEL_NAME_TX_TYPE, TRANSACTIONS_RECEIVED.get_name()),
    PanelType::Stat,
);

const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_SOURCE: Panel = Panel::new(
    TRANSACTIONS_RECEIVED.get_name(),
    TRANSACTIONS_RECEIVED.get_description(),
    formatcp!("sum  by ({}) ({}) ", LABEL_NAME_SOURCE, TRANSACTIONS_RECEIVED.get_name()),
    PanelType::Stat,
);

const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_RATE: Panel = Panel::new(
    "gateway_transactions_received_rate (TPS)",
    "The rate of transactions received by the gateway during the last 20 minutes",
    formatcp!("sum(rate({}[20m]))", TRANSACTIONS_RECEIVED.get_name()),
    PanelType::Graph,
);

const PANEL_GATEWAY_ADD_TX_LATENCY: Panel = Panel::new(
    GATEWAY_ADD_TX_LATENCY.get_name(),
    GATEWAY_ADD_TX_LATENCY.get_description(),
    formatcp!("avg_over_time({}[2m])", GATEWAY_ADD_TX_LATENCY.get_name()),
    PanelType::Graph,
);

const PANEL_GATEWAY_VALIDATE_TX_LATENCY: Panel = Panel::new(
    GATEWAY_VALIDATE_TX_LATENCY.get_name(),
    GATEWAY_VALIDATE_TX_LATENCY.get_description(),
    formatcp!("avg_over_time({}[2m])", GATEWAY_VALIDATE_TX_LATENCY.get_name()),
    PanelType::Graph,
);

const PANEL_GATEWAY_TRANSACTIONS_FAILED: Panel = Panel::new(
    TRANSACTIONS_FAILED.get_name(),
    TRANSACTIONS_FAILED.get_description(),
    formatcp!("sum  by ({}) ({})", GATEWAY_LABEL_NAME_TX_TYPE, TRANSACTIONS_FAILED.get_name()),
    PanelType::Stat,
);

const PANEL_GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL: Panel = Panel::new(
    TRANSACTIONS_SENT_TO_MEMPOOL.get_name(),
    TRANSACTIONS_SENT_TO_MEMPOOL.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        GATEWAY_LABEL_NAME_TX_TYPE,
        TRANSACTIONS_SENT_TO_MEMPOOL.get_name()
    ),
    PanelType::Stat,
);

const PANEL_L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, PanelType::Stat);

const PANEL_MEMPOOL_TRANSACTIONS_RECEIVED: Panel = Panel::new(
    MEMPOOL_TRANSACTIONS_RECEIVED.get_name(),
    MEMPOOL_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        MEMPOOL_LABEL_NAME_TX_TYPE,
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name()
    ),
    PanelType::Stat,
);

const PANEL_MEMPOOL_TRANSACTIONS_RECEIVED_RATE: Panel = Panel::new(
    "mempool_transactions_received_rate (TPS)",
    "The rate of transactions received by the mempool during the last 20 minutes",
    formatcp!("sum(rate({}[20m]))", MEMPOOL_TRANSACTIONS_RECEIVED.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_TRANSACTIONS_COMMITTED: Panel =
    Panel::from_counter(MEMPOOL_TRANSACTIONS_COMMITTED, PanelType::Stat);

const PANEL_MEMPOOL_TRANSACTIONS_DROPPED: Panel = Panel::new(
    MEMPOOL_TRANSACTIONS_DROPPED.get_name(),
    MEMPOOL_TRANSACTIONS_DROPPED.get_description(),
    formatcp!("sum  by ({}) ({})", LABEL_NAME_DROP_REASON, MEMPOOL_TRANSACTIONS_DROPPED.get_name()),
    PanelType::Stat,
);

const PANEL_MEMPOOL_POOL_SIZE: Panel = Panel::new(
    MEMPOOL_POOL_SIZE.get_name(),
    "The average size of the pool",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_POOL_SIZE.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_PRIORITY_QUEUE_SIZE: Panel = Panel::new(
    MEMPOOL_PRIORITY_QUEUE_SIZE.get_name(),
    "The average size of the priority queue",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_PRIORITY_QUEUE_SIZE.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_PENDING_QUEUE_SIZE: Panel = Panel::new(
    MEMPOOL_PENDING_QUEUE_SIZE.get_name(),
    "The average size of the pending queue",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_PENDING_QUEUE_SIZE.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_TOTAL_SIZE_IN_BYTES: Panel = Panel::new(
    MEMPOOL_TOTAL_SIZE_BYTES.get_name(),
    "The average total transaction size in bytes over time in the mempool",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_TOTAL_SIZE_BYTES.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_GET_TXS_SIZE: Panel = Panel::new(
    MEMPOOL_GET_TXS_SIZE.get_name(),
    "The average size of the get_txs",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_GET_TXS_SIZE.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_DELAYED_DECLARES_SIZE: Panel = Panel::new(
    MEMPOOL_DELAYED_DECLARES_SIZE.get_name(),
    "The average number of delayed declare transactions",
    formatcp!("avg_over_time({}[2m])", MEMPOOL_DELAYED_DECLARES_SIZE.get_name()),
    PanelType::Graph,
);

const PANEL_MEMPOOL_TRANSACTION_TIME_SPENT: Panel = Panel::new(
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_name(),
    TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_description(),
    formatcp!("avg_over_time({}[2m])", TRANSACTION_TIME_SPENT_IN_MEMPOOL.get_name()),
    PanelType::Graph,
);

const PANEL_APOLLO_STATE_READER_CLASS_CACHE_MISS_RATIO: Panel = Panel::new(
    "class_cache_miss_ratio",
    "The ratio of cache misses when requesting compiled classes from the apollo state reader",
    formatcp!(
        "100 * ({} / max(({} + {}), 1))",
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_HITS.get_name()
    ),
    PanelType::Graph,
);
const PANEL_APOLLO_STATE_READER_NATIVE_CLASS_RETURNED_RATIO: Panel = Panel::new(
    "native_class_returned_ratio",
    "The ratio of Native classes returned by the apollo state reader",
    formatcp!(
        "100 * ({} / max(({} + {}), 1))",
        NATIVE_CLASS_RETURNED.get_name(),
        CLASS_CACHE_HITS.get_name(),
        CLASS_CACHE_MISSES.get_name()
    ),
    PanelType::Graph,
);

const MEMPOOL_P2P_ROW: Row = Row::new(
    "MempoolP2p",
    "Mempool peer to peer metrics",
    &[
        PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS,
        PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES,
        PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
        PANEL_MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    ],
);

const CONSENSUS_P2P_ROW: Row = Row::new(
    "ConsensusP2p",
    "Consensus peer to peer metrics",
    &[
        PANEL_CONSENSUS_NUM_CONNECTED_PEERS,
        PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES,
        PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
        PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
        PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    ],
);

const STATE_SYNC_P2P_ROW: Row = Row::new(
    "StateSyncP2p",
    "State sync peer to peer metrics",
    &[
        PANEL_STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
        PANEL_STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
        PANEL_STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
    ],
);

const BATCHER_ROW: Row = Row::new(
    "Batcher",
    "Batcher metrics including proposals and transactions",
    &[
        PANEL_PROPOSAL_STARTED,
        PANEL_PROPOSAL_SUCCEEDED,
        PANEL_PROPOSAL_FAILED,
        PANEL_BATCHED_TRANSACTIONS,
    ],
);

const APOLLO_STATE_READER_ROW: Row = Row::new(
    "Apollo State Reader",
    "Apollo state reader metrics",
    &[
        PANEL_APOLLO_STATE_READER_CLASS_CACHE_MISS_RATIO,
        PANEL_APOLLO_STATE_READER_NATIVE_CLASS_RETURNED_RATIO,
    ],
);

const CONSENSUS_ROW: Row = Row::new(
    "Consensus",
    "Consensus metrics including block number, round, and so on.",
    &[
        PANEL_CONSENSUS_BLOCK_NUMBER,
        PANEL_CONSENSUS_ROUND,
        PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
        PANEL_CONSENSUS_CACHED_VOTES,
        PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
        PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC,
        PANEL_CONSENSUS_PROPOSALS_RECEIVED,
        PANEL_CONSENSUS_PROPOSALS_VALID_INIT,
        PANEL_CONSENSUS_PROPOSALS_VALIDATED,
        PANEL_CONSENSUS_PROPOSALS_INVALID,
        PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL,
        PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED,
        PANEL_CONSENSUS_REPROPOSALS,
        PANEL_CONSENSUS_NEW_VALUE_LOCKS,
        PANEL_CONSENSUS_HELD_LOCKS,
        PANEL_CONSENSUS_TIMEOUTS_BY_TYPE,
        PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
        PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL,
    ],
);

const HTTP_SERVER_ROW: Row = Row::new(
    "Http Server",
    "Http Server metrics including added transactions",
    &[PANEL_ADDED_TRANSACTIONS_TOTAL],
);

pub const GATEWAY_ROW: Row = Row::new(
    "Gateway",
    "Gateway metrics",
    &[
        PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_TYPE,
        PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_SOURCE,
        PANEL_GATEWAY_TRANSACTIONS_RECEIVED_RATE,
        PANEL_GATEWAY_ADD_TX_LATENCY,
        PANEL_GATEWAY_VALIDATE_TX_LATENCY,
        PANEL_GATEWAY_TRANSACTIONS_FAILED,
        PANEL_GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    ],
);

pub const L1_GAS_PRICE_ROW: Row = Row::new(
    "L1 Gas Price",
    "L1 gas price provider and scraper metrics",
    &[PANEL_L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY],
);

pub const MEMPOOL_ROW: Row = Row::new(
    "Mempool",
    "Mempool metrics",
    &[
        PANEL_MEMPOOL_TRANSACTIONS_RECEIVED,
        PANEL_MEMPOOL_TRANSACTIONS_RECEIVED_RATE,
        PANEL_MEMPOOL_TRANSACTIONS_DROPPED,
        PANEL_MEMPOOL_TRANSACTIONS_COMMITTED,
        PANEL_MEMPOOL_POOL_SIZE,
        PANEL_MEMPOOL_PRIORITY_QUEUE_SIZE,
        PANEL_MEMPOOL_PENDING_QUEUE_SIZE,
        PANEL_MEMPOOL_TOTAL_SIZE_IN_BYTES,
        PANEL_MEMPOOL_GET_TXS_SIZE,
        PANEL_MEMPOOL_DELAYED_DECLARES_SIZE,
        PANEL_MEMPOOL_TRANSACTION_TIME_SPENT,
    ],
);

pub const SEQUENCER_DASHBOARD: Dashboard = Dashboard::new(
    "Sequencer Node Dashboard",
    "Monitoring of the decentralized sequencer node",
    &[
        BATCHER_ROW,
        CONSENSUS_ROW,
        HTTP_SERVER_ROW,
        MEMPOOL_P2P_ROW,
        CONSENSUS_P2P_ROW,
        STATE_SYNC_P2P_ROW,
        GATEWAY_ROW,
        MEMPOOL_ROW,
        APOLLO_STATE_READER_ROW,
    ],
);
