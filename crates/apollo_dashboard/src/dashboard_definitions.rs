use apollo_compile_to_casm::metrics::COMPILATION_DURATION;
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_infra::metrics::{
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
    STATE_SYNC_LOCAL_MSGS_PROCESSED,
    STATE_SYNC_LOCAL_MSGS_RECEIVED,
    STATE_SYNC_LOCAL_QUEUE_DEPTH,
    STATE_SYNC_REMOTE_MSGS_PROCESSED,
    STATE_SYNC_REMOTE_MSGS_RECEIVED,
    STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_state_reader::metrics::{
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    STATE_READER_METRIC_RATE_DURATION,
};
use apollo_state_sync_metrics::metrics::{
    P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    P2P_SYNC_NUM_CONNECTED_PEERS,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_REVERTED_TRANSACTIONS,
};
use const_format::formatcp;

use crate::dashboard::{Dashboard, Panel, PanelType, Row};
use crate::panels::batcher::{
    PANEL_BATCHED_TRANSACTIONS,
    PANEL_BATCHER_LOCAL_MSGS_PROCESSED,
    PANEL_BATCHER_LOCAL_MSGS_RECEIVED,
    PANEL_BATCHER_LOCAL_QUEUE_DEPTH,
    PANEL_BATCHER_REMOTE_MSGS_PROCESSED,
    PANEL_BATCHER_REMOTE_MSGS_RECEIVED,
    PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED,
    PANEL_PROPOSAL_FAILED,
    PANEL_PROPOSAL_STARTED,
    PANEL_PROPOSAL_SUCCEEDED,
};
use crate::panels::class_manager::{
    PANEL_CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
    PANEL_CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
    PANEL_CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
    PANEL_CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    PANEL_CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
    PANEL_CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
};
use crate::panels::consensus::{
    PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    PANEL_CENDE_WRITE_BLOB_FAILURE,
    PANEL_CENDE_WRITE_BLOB_SUCCESS,
    PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    PANEL_CONSENSUS_BLOCK_NUMBER,
    PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED,
    PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL,
    PANEL_CONSENSUS_CACHED_VOTES,
    PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    PANEL_CONSENSUS_HELD_LOCKS,
    PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    PANEL_CONSENSUS_NEW_VALUE_LOCKS,
    PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    PANEL_CONSENSUS_NUM_CONNECTED_PEERS,
    PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL,
    PANEL_CONSENSUS_PROPOSALS_INVALID,
    PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    PANEL_CONSENSUS_PROPOSALS_RECEIVED,
    PANEL_CONSENSUS_PROPOSALS_VALIDATED,
    PANEL_CONSENSUS_PROPOSALS_VALID_INIT,
    PANEL_CONSENSUS_REPROPOSALS,
    PANEL_CONSENSUS_ROUND,
    PANEL_CONSENSUS_TIMEOUTS_BY_TYPE,
    PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use crate::panels::gateway::{
    PANEL_GATEWAY_ADD_TX_LATENCY,
    PANEL_GATEWAY_LOCAL_MSGS_PROCESSED,
    PANEL_GATEWAY_LOCAL_MSGS_RECEIVED,
    PANEL_GATEWAY_LOCAL_QUEUE_DEPTH,
    PANEL_GATEWAY_REMOTE_MSGS_PROCESSED,
    PANEL_GATEWAY_REMOTE_MSGS_RECEIVED,
    PANEL_GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
    PANEL_GATEWAY_TRANSACTIONS_FAILED,
    PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_SOURCE,
    PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_TYPE,
    PANEL_GATEWAY_TRANSACTIONS_RECEIVED_RATE,
    PANEL_GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    PANEL_GATEWAY_VALIDATE_TX_LATENCY,
};
use crate::panels::l1_gas_price::{
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use crate::panels::l1_provider::{
    PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use crate::panels::mempool::{
    PANEL_MEMPOOL_DELAYED_DECLARES_SIZE,
    PANEL_MEMPOOL_GET_TXS_SIZE,
    PANEL_MEMPOOL_LOCAL_MSGS_PROCESSED,
    PANEL_MEMPOOL_LOCAL_MSGS_RECEIVED,
    PANEL_MEMPOOL_LOCAL_QUEUE_DEPTH,
    PANEL_MEMPOOL_PENDING_QUEUE_SIZE,
    PANEL_MEMPOOL_POOL_SIZE,
    PANEL_MEMPOOL_PRIORITY_QUEUE_SIZE,
    PANEL_MEMPOOL_REMOTE_MSGS_PROCESSED,
    PANEL_MEMPOOL_REMOTE_MSGS_RECEIVED,
    PANEL_MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
    PANEL_MEMPOOL_TOTAL_SIZE_IN_BYTES,
    PANEL_MEMPOOL_TRANSACTIONS_COMMITTED,
    PANEL_MEMPOOL_TRANSACTIONS_DROPPED,
    PANEL_MEMPOOL_TRANSACTIONS_RECEIVED,
    PANEL_MEMPOOL_TRANSACTIONS_RECEIVED_RATE,
    PANEL_MEMPOOL_TRANSACTION_TIME_SPENT,
};
use crate::panels::mempool_p2p::{
    PANEL_MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    PANEL_MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    PANEL_MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    PANEL_MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES,
    PANEL_MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    PANEL_MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    PANEL_MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
};

#[cfg(test)]
#[path = "dashboard_definitions_test.rs"]
mod dashboard_definitions_test;

pub const DEV_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana.json";

const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel =
    Panel::from_counter(ADDED_TRANSACTIONS_TOTAL, PanelType::Stat);

const PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(SIERRA_COMPILER_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_STATE_SYNC_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_STATE_SYNC_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(STATE_SYNC_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_STATE_SYNC_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_STATE_SYNC_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(STATE_SYNC_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_STATE_SYNC_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(STATE_SYNC_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_P2P_SYNC_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_CONNECTED_PEERS, PanelType::Stat);
const PANEL_P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS, PanelType::Stat);
const PANEL_P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS: Panel =
    Panel::from_gauge(P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS, PanelType::Stat);
const PANEL_STATE_SYNC_PROCESSED_TRANSACTIONS: Panel =
    Panel::from_counter(STATE_SYNC_PROCESSED_TRANSACTIONS, PanelType::Stat);
const PANEL_STATE_SYNC_REVERTED_TRANSACTIONS: Panel =
    Panel::from_counter(STATE_SYNC_REVERTED_TRANSACTIONS, PanelType::Stat);

const PANEL_APOLLO_STATE_READER_CLASS_CACHE_MISS_RATIO: Panel = Panel::new(
    "class_cache_miss_ratio",
    "The ratio of cache misses when requesting compiled classes from the apollo state reader",
    formatcp!(
        "100 * (rate({}[{}]) / (rate({}[{}]) + rate({}[{}])))",
        CLASS_CACHE_MISSES.get_name(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_MISSES.get_name(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_HITS.get_name(),
        STATE_READER_METRIC_RATE_DURATION
    ),
    PanelType::Graph,
);
const PANEL_APOLLO_STATE_READER_NATIVE_CLASS_RETURNED_RATIO: Panel = Panel::new(
    "native_class_returned_ratio",
    "The ratio of Native classes returned by the apollo state reader",
    formatcp!(
        "100 * (rate({}[{}]) / (rate({}[{}]) + rate({}[{}])))",
        NATIVE_CLASS_RETURNED.get_name(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_HITS.get_name(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_MISSES.get_name(),
        STATE_READER_METRIC_RATE_DURATION,
    ),
    PanelType::Graph,
);

const PANEL_COMPILATION_DURATION: Panel = Panel::new(
    COMPILATION_DURATION.get_name(),
    COMPILATION_DURATION.get_description(),
    formatcp!("avg_over_time({}[2m])", COMPILATION_DURATION.get_name()),
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

const MEMPOOL_P2P_INFRA_ROW: Row = Row::new(
    "MempoolP2pInfra",
    "Mempool peer to peer infra metrics",
    &[
        PANEL_MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
        PANEL_MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
        PANEL_MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
        PANEL_MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
        PANEL_MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    ],
);

const SIERRA_COMPILER_INFRA_ROW: Row = Row::new(
    "SierraCompilerInfra",
    "sierra compiler infra metrics",
    &[
        PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
        PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
        PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
        PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
        PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    ],
);

const STATE_SYNC_INFRA_ROW: Row = Row::new(
    "StateSyncInfra",
    "state sync infra metrics",
    &[
        PANEL_STATE_SYNC_LOCAL_MSGS_RECEIVED,
        PANEL_STATE_SYNC_LOCAL_MSGS_PROCESSED,
        PANEL_STATE_SYNC_LOCAL_QUEUE_DEPTH,
        PANEL_STATE_SYNC_REMOTE_MSGS_RECEIVED,
        PANEL_STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_STATE_SYNC_REMOTE_MSGS_PROCESSED,
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
        PANEL_P2P_SYNC_NUM_CONNECTED_PEERS,
        PANEL_P2P_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
        PANEL_P2P_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
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

const BATCHER_INFRA_ROW: Row = Row::new(
    "Batcher Infra",
    "Batcher infra metrics",
    &[
        PANEL_BATCHER_LOCAL_MSGS_RECEIVED,
        PANEL_BATCHER_LOCAL_MSGS_PROCESSED,
        PANEL_BATCHER_LOCAL_QUEUE_DEPTH,
        PANEL_BATCHER_REMOTE_MSGS_RECEIVED,
        PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_BATCHER_REMOTE_MSGS_PROCESSED,
    ],
);

const CLASS_MANAGER_INFRA_ROW: Row = Row::new(
    "Class Manager Infra",
    "Class manager infra metrics",
    &[
        PANEL_CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
        PANEL_CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
        PANEL_CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
        PANEL_CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
        PANEL_CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    ],
);

const L1_PROVIDER_INFRA_ROW: Row = Row::new(
    "L1 Provider Infra",
    "L1 provider infra metrics",
    &[
        PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED,
        PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED,
        PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH,
        PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED,
        PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    ],
);

const L1_GAS_PRICE_INFRA_ROW: Row = Row::new(
    "L1 Gas Price Infra",
    "L1 gas price infra metrics",
    &[
        PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
        PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
        PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
        PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
        PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
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
        PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
        PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
        PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
        PANEL_CENDE_WRITE_BLOB_SUCCESS,
        PANEL_CENDE_WRITE_BLOB_FAILURE,
    ],
);

const HTTP_SERVER_ROW: Row = Row::new(
    "Http Server",
    "Http Server metrics including added transactions",
    &[PANEL_ADDED_TRANSACTIONS_TOTAL],
);

const STATE_SYNC_ROW: Row = Row::new(
    "State Sync",
    "State sync metrics",
    &[PANEL_STATE_SYNC_PROCESSED_TRANSACTIONS, PANEL_STATE_SYNC_REVERTED_TRANSACTIONS],
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

pub const GATEWAY_INFRA_ROW: Row = Row::new(
    "Gateway Infra",
    "Gateway infra metrics",
    &[
        PANEL_GATEWAY_LOCAL_MSGS_RECEIVED,
        PANEL_GATEWAY_LOCAL_MSGS_PROCESSED,
        PANEL_GATEWAY_LOCAL_QUEUE_DEPTH,
        PANEL_GATEWAY_REMOTE_MSGS_RECEIVED,
        PANEL_GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_GATEWAY_REMOTE_MSGS_PROCESSED,
    ],
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

pub const MEMPOOL_INFRA_ROW: Row = Row::new(
    "Mempool Infra",
    "Mempool infra metrics",
    &[
        PANEL_MEMPOOL_LOCAL_MSGS_RECEIVED,
        PANEL_MEMPOOL_LOCAL_MSGS_PROCESSED,
        PANEL_MEMPOOL_LOCAL_QUEUE_DEPTH,
        PANEL_MEMPOOL_REMOTE_MSGS_RECEIVED,
        PANEL_MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
        PANEL_MEMPOOL_REMOTE_MSGS_PROCESSED,
    ],
);

pub const COMPILE_TO_CASM_ROW: Row =
    Row::new("Compile sierra to casm", "Compile to casm metrics", &[PANEL_COMPILATION_DURATION]);

pub const SEQUENCER_DASHBOARD: Dashboard = Dashboard::new(
    "Sequencer Node Dashboard",
    "Monitoring of the decentralized sequencer node",
    &[
        BATCHER_ROW,
        CONSENSUS_ROW,
        HTTP_SERVER_ROW,
        STATE_SYNC_ROW,
        MEMPOOL_P2P_ROW,
        CONSENSUS_P2P_ROW,
        STATE_SYNC_P2P_ROW,
        GATEWAY_ROW,
        MEMPOOL_ROW,
        APOLLO_STATE_READER_ROW,
        BATCHER_INFRA_ROW,
        GATEWAY_INFRA_ROW,
        CLASS_MANAGER_INFRA_ROW,
        L1_PROVIDER_INFRA_ROW,
        L1_GAS_PRICE_INFRA_ROW,
        MEMPOOL_INFRA_ROW,
        MEMPOOL_P2P_INFRA_ROW,
        SIERRA_COMPILER_INFRA_ROW,
        COMPILE_TO_CASM_ROW,
        STATE_SYNC_INFRA_ROW,
    ],
);
