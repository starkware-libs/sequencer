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
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};
use apollo_gateway::metrics::{
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_TRANSACTIONS_FAILED,
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    GATEWAY_VALIDATE_TX_LATENCY,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE as GATEWAY_LABEL_NAME_TX_TYPE,
};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_infra::metrics::{
    BATCHER_LOCAL_MSGS_PROCESSED,
    BATCHER_LOCAL_MSGS_RECEIVED,
    BATCHER_LOCAL_QUEUE_DEPTH,
    BATCHER_REMOTE_MSGS_PROCESSED,
    BATCHER_REMOTE_MSGS_RECEIVED,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
    CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
    CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
    CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
    CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
    CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
    GATEWAY_LOCAL_MSGS_PROCESSED,
    GATEWAY_LOCAL_MSGS_RECEIVED,
    GATEWAY_LOCAL_QUEUE_DEPTH,
    GATEWAY_REMOTE_MSGS_PROCESSED,
    GATEWAY_REMOTE_MSGS_RECEIVED,
    GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
    L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
    MEMPOOL_LOCAL_MSGS_PROCESSED,
    MEMPOOL_LOCAL_MSGS_RECEIVED,
    MEMPOOL_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
    MEMPOOL_REMOTE_MSGS_PROCESSED,
    MEMPOOL_REMOTE_MSGS_RECEIVED,
    MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
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
use apollo_state_reader::metrics::{
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    STATE_READER_METRIC_RATE_DURATION,
};
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
const PANEL_BATCHER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_BATCHER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_BATCHER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_BATCHER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(BATCHER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_BATCHER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(BATCHER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_BATCHER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(BATCHER_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_CLASS_MANAGER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_CLASS_MANAGER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_CLASS_MANAGER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_CLASS_MANAGER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_CLASS_MANAGER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(CLASS_MANAGER_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(L1_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::Stat);

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
const PANEL_CONSENSUS_L2_GAS_PRICE: Panel =
    Panel::from_gauge(CONSENSUS_L2_GAS_PRICE, PanelType::Stat);
const PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER: Panel =
    Panel::from_gauge(CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER, PanelType::Stat);
const PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY: Panel = Panel::new(
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_name(),
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_description(),
    formatcp!("avg_over_time({}[2m])", CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY.get_name()),
    PanelType::Stat,
);
const PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY: Panel = Panel::new(
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name(),
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_description(),
    formatcp!("avg_over_time({}[2m])", CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name()),
    PanelType::Stat,
);
const PANEL_MEMPOOL_P2P_NUM_CONNECTED_PEERS: Panel =
    Panel::from_gauge(MEMPOOL_P2P_NUM_CONNECTED_PEERS, PanelType::Stat);
const PANEL_MEMPOOL_P2P_NUM_SENT_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_SENT_MESSAGES, PanelType::Stat);
const PANEL_MEMPOOL_P2P_NUM_RECEIVED_MESSAGES: Panel =
    Panel::from_counter(MEMPOOL_P2P_NUM_RECEIVED_MESSAGES, PanelType::Stat);
const PANEL_MEMPOOL_P2P_BROADCASTED_BATCH_SIZE: Panel =
    Panel::from_hist(MEMPOOL_P2P_BROADCASTED_BATCH_SIZE, PanelType::Stat);

const PANEL_MEMPOOL_P2P_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_P2P_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_P2P_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_MEMPOOL_P2P_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_P2P_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_P2P_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_MEMPOOL_P2P_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(MEMPOOL_P2P_LOCAL_QUEUE_DEPTH, PanelType::Stat);

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
    GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
    GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!(
        "sum  by ({}) ({}) ",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_RECEIVED.get_name()
    ),
    PanelType::Stat,
);

const PANEL_GATEWAY_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_GATEWAY_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(GATEWAY_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_GATEWAY_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_GATEWAY_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_GATEWAY_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(GATEWAY_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_GATEWAY_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(GATEWAY_LOCAL_QUEUE_DEPTH, PanelType::Stat);

const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_BY_SOURCE: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_RECEIVED.get_name(),
    GATEWAY_TRANSACTIONS_RECEIVED.get_description(),
    formatcp!("sum  by ({}) ({}) ", LABEL_NAME_SOURCE, GATEWAY_TRANSACTIONS_RECEIVED.get_name()),
    PanelType::Stat,
);

const PANEL_GATEWAY_TRANSACTIONS_RECEIVED_RATE: Panel = Panel::new(
    "gateway_transactions_received_rate (TPS)",
    "The rate of transactions received by the gateway during the last 20 minutes",
    formatcp!("sum(rate({}[20m]))", GATEWAY_TRANSACTIONS_RECEIVED.get_name()),
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
    GATEWAY_TRANSACTIONS_FAILED.get_name(),
    GATEWAY_TRANSACTIONS_FAILED.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_FAILED.get_name()
    ),
    PanelType::Stat,
);

const PANEL_GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL: Panel = Panel::new(
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name(),
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_description(),
    formatcp!(
        "sum  by ({}) ({})",
        GATEWAY_LABEL_NAME_TX_TYPE,
        GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.get_name()
    ),
    PanelType::Stat,
);

const PANEL_MEMPOOL_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_LOCAL_MSGS_PROCESSED, PanelType::Stat);
const PANEL_MEMPOOL_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
const PANEL_MEMPOOL_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(MEMPOOL_REMOTE_MSGS_PROCESSED, PanelType::Stat);
const PANEL_MEMPOOL_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(MEMPOOL_LOCAL_QUEUE_DEPTH, PanelType::Stat);

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
        PANEL_CONSENSUS_L2_GAS_PRICE,
        PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
        PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
        PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
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
        BATCHER_INFRA_ROW,
        GATEWAY_INFRA_ROW,
        CLASS_MANAGER_INFRA_ROW,
        L1_PROVIDER_INFRA_ROW,
        L1_GAS_PRICE_INFRA_ROW,
        MEMPOOL_INFRA_ROW,
        MEMPOOL_P2P_INFRA_ROW,
        SIERRA_COMPILER_INFRA_ROW,
        STATE_SYNC_INFRA_ROW,
    ],
);
