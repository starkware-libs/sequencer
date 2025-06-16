use apollo_batcher::metrics::{BATCHED_TRANSACTIONS, LAST_BATCHED_BLOCK};
use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_INBOUND_STREAM_EVICTED,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_ROUND,
    CONSENSUS_ROUND_ABOVE_ZERO,
};
use apollo_consensus_manager::metrics::CONSENSUS_VOTES_NUM_SENT_MESSAGES;
use apollo_consensus_orchestrator::metrics::{
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR,
};
use apollo_gateway::metrics::{GATEWAY_ADD_TX_LATENCY, GATEWAY_TRANSACTIONS_RECEIVED};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_l1_gas_price::metrics::{
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
};
use apollo_l1_provider::metrics::L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT;
use apollo_mempool::metrics::{
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_RECEIVED,
};
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
};
use blockifier::metrics::NATIVE_COMPILATION_ERROR;
use const_format::formatcp;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    Alerts,
};

// TODO(Tsabary): this file should be managed by this crate, hence should be moved here to a
// resources folder.
pub const DEV_ALERTS_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana_alerts.json";
pub const PROMETHEUS_EPSILON: f64 = 0.0001;

const CONSENSUS_BLOCK_NUMBER_STUCK: Alert = Alert {
    name: "consensus_block_number_stuck",
    title: "Consensus block number stuck",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("changes({}[5m])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 10.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1s",
    evaluation_interval_sec: 10,
    severity: AlertSeverity::Regular,
};

// If this happens, we expect to also see other nodes alert on `consensus_validate_proposal_failed`.
const CONSENSUS_BUILD_PROPOSAL_FAILED_ALERT: Alert = Alert {
    name: "consensus_build_proposal_failed",
    title: "Consensus build proposal failed",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1h])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 10.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "10s",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::DayOnly,
};

const CONSENSUS_BUILD_PROPOSAL_FAILED_ONCE_ALERT: Alert = Alert {
    name: "consensus_build_proposal_failed_once",
    title: "Consensus build proposal failed once",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1m])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "10s",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const CONSENSUS_VALIDATE_PROPOSAL_FAILED_ALERT: Alert = Alert {
    name: "consensus_validate_proposal_failed",
    title: "Consensus validate proposal failed",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1h])", CONSENSUS_PROPOSALS_INVALID.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 10.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::DayOnly,
};

const CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS_RATIO: Alert = Alert {
    name: "consensus_decisions_reached_by_consensus_ratio",
    title: "Consensus decisions reached by consensus ratio",
    alert_group: AlertGroup::Consensus,
    // Clamp to avoid divide by 0.
    expr: formatcp!(
        "increase({}[10m]) / clamp_min(increase({}[10m]) + increase({}[10m]), 1)",
        CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter(),
        CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter(),
        CONSENSUS_DECISIONS_REACHED_BY_SYNC.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.5,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::WorkingHours,
};

const CONSENSUS_INBOUND_STREAM_EVICTED_ALERT: Alert = Alert {
    name: "consensus_inbound_stream_evicted",
    title: "Consensus inbound stream evicted",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1h])", CONSENSUS_INBOUND_STREAM_EVICTED.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const CONSENSUS_VOTES_NUM_SENT_MESSAGES_ALERT: Alert = Alert {
    name: "consensus_votes_num_sent_messages",
    title: "Consensus votes num sent messages",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[20m])", CONSENSUS_VOTES_NUM_SENT_MESSAGES.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 20.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY_TOO_HIGH: Alert = Alert {
    name: "cende_write_prev_height_blob_latency_too_high",
    title: "Cende write prev height blob latency too high",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!(
        "avg_over_time({}[20m])",
        CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        // This is 50% of the proposal timeout.
        comparison_value: 1.5,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::WorkingHours,
};

const CENDE_WRITE_BLOB_FAILURE_ALERT: Alert = Alert {
    name: "cende_write_blob_failure",
    title: "Cende write blob failure",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 10.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::DayOnly,
};

const CENDE_WRITE_BLOB_FAILURE_ONCE_ALERT: Alert = Alert {
    name: "cende_write_blob_failure_once",
    title: "Cende write blob failure once",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR_RATE: Alert = Alert {
    name: "consensus_l1_gas_price_provider_error_rate",
    title: "Consensus L1 gas price provider error rate",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("rate({}[1h])", CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0 / 3600.0, // 5 per hour
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::WorkingHours,
};

const CONSENSUS_ROUND_ABOVE_ZERO_ALERT: Alert = Alert {
    name: "consensus_round_above_zero",
    title: "Consensus round above zero",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("rate({}[1h])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0 / 3600.0, // 5 per hour
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const CONSENSUS_CONFLICTING_VOTES_RATE: Alert = Alert {
    name: "consensus_conflicting_votes_rate",
    title: "Consensus conflicting votes rate",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("rate({}[20m])", CONSENSUS_CONFLICTING_VOTES.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const GATEWAY_ADD_TX_RATE_DROP: Alert = Alert {
    name: "gateway_add_tx_rate_drop",
    title: "Gateway add_tx rate drop",
    alert_group: AlertGroup::Gateway,
    expr: formatcp!(
        "sum(rate({}[20m])) or vector(0)",
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.1,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const GATEWAY_ADD_TX_LATENCY_INCREASE: Alert = Alert {
    name: "gateway_add_tx_latency_increase",
    title: "Gateway avg add_tx latency increase",
    alert_group: AlertGroup::Gateway,
    expr: formatcp!(
        "sum(rate({}[1m]))/sum(rate({}[1m]))",
        GATEWAY_ADD_TX_LATENCY.get_name_sum_with_filter(),
        GATEWAY_ADD_TX_LATENCY.get_name_count_with_filter(),
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 2.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const MEMPOOL_ADD_TX_RATE_DROP: Alert = Alert {
    name: "mempool_add_tx_rate_drop",
    title: "Mempool add_tx rate drop",
    alert_group: AlertGroup::Mempool,
    expr: formatcp!(
        "sum(rate({}[20m])) or vector(0)",
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.1,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const HTTP_SERVER_IDLE: Alert = Alert {
    name: "http_server_idle",
    title: "http server idle",
    alert_group: AlertGroup::HttpServer,
    expr: formatcp!("rate(max({})[60m:])", ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.000001,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "5m",
    evaluation_interval_sec: 60,
    severity: AlertSeverity::Regular,
};

const L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT_ALERT: Alert = Alert {
    name: "l1_message_scraper_baselayer_error_count",
    title: "L1 message scraper baselayer error count",
    alert_group: AlertGroup::L1GasPrice,
    expr: formatcp!(
        "rate({}[1h])",
        L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0 / 3600.0, // 5 per hour
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY_ALERT: Alert = Alert {
    name: "l1_gas_price_provider_insufficient_history",
    title: "L1 gas price provider insufficient history",
    alert_group: AlertGroup::L1GasPrice,
    expr: formatcp!(
        "rate({}[1m])",
        L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const L1_GAS_PRICE_REORG_DETECTED_ALERT: Alert = Alert {
    name: "l1_gas_price_scraper_reorg_detected",
    title: "L1 gas price scraper reorg detected",
    alert_group: AlertGroup::L1GasPrice,
    expr: formatcp!("rate({}[1m])", L1_GAS_PRICE_SCRAPER_REORG_DETECTED.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT_ALERT: Alert = Alert {
    name: "l1_message_scraper_baselayer_error_count",
    title: "L1 message scraper baselayer error count",
    alert_group: AlertGroup::L1Messages,
    expr: formatcp!(
        "rate({}[1h])",
        L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0 / 3600.0, // 5 per hour
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const L1_MESSAGE_SCRAPER_REORG_DETECTED_ALERT: Alert = Alert {
    name: "l1_message_scraper_reorg_detected",
    title: "L1 message scraper reorg detected",
    alert_group: AlertGroup::L1Messages,
    expr: formatcp!(
        "rate({}[1m])",
        L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

// The rate of add_txs is lower than the rate of transactions inserted into a block since this node
// is not always the proposer.
const MEMPOOL_GET_TXS_SIZE_DROP: Alert = Alert {
    name: "mempool_get_txs_size_drop",
    title: "Mempool get_txs size drop",
    alert_group: AlertGroup::Mempool,
    // TODO(Tsabary): revisit this panel, it used to be defined with "avg_over_time({}[20m])".
    expr: formatcp!("avg_over_time({}[20m])", MEMPOOL_GET_TXS_SIZE.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.01,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const MEMPOOL_POOL_SIZE_INCREASE: Alert = Alert {
    name: "mempool_pool_size_increase",
    title: "Mempool pool size increase",
    alert_group: AlertGroup::Mempool,
    expr: formatcp!("{}", MEMPOOL_POOL_SIZE.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 2000.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const CONSENSUS_ROUND_HIGH_AVG: Alert = Alert {
    name: "consensus_round_high_avg",
    title: "Consensus round high average",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("avg_over_time({}[10m])", CONSENSUS_ROUND.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.2,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const NATIVE_COMPILATION_ERROR_INCREASE: Alert = Alert {
    name: "native_compilation_error",
    title: "Native compilation alert",
    alert_group: AlertGroup::Batcher,
    expr: formatcp!("increase({}[1m])", NATIVE_COMPILATION_ERROR.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Informational,
};

const STATE_SYNC_LAG: Alert = Alert {
    name: "state_sync_lag",
    title: "State sync lag",
    alert_group: AlertGroup::StateSync,
    expr: formatcp!(
        "min_over_time(({} - {})[3m])",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
        STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
    ), // Alert when the central sync is ahead of the class manager by more than 5 blocks
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "3m",
    evaluation_interval_sec: 20,
    severity: AlertSeverity::Regular,
};

const STATE_SYNC_STUCK: Alert = Alert {
    name: "state_sync_stuck",
    title: "State sync stuck",
    alert_group: AlertGroup::StateSync,
    expr: formatcp!("rate({}[1m])", STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()), /* Alert is triggered when the class manager marker is not updated for 1m */
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: PROMETHEUS_EPSILON,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "3m",
    evaluation_interval_sec: 60,
    severity: AlertSeverity::Regular,
};

const BATCHED_TRANSACTIONS_STUCK: Alert = Alert {
    name: "batched_transactions_stuck",
    title: "Batched transactions stuck",
    alert_group: AlertGroup::Batcher,
    expr: formatcp!("changes({}[5m])", BATCHED_TRANSACTIONS.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 1.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1s",
    evaluation_interval_sec: 10,
    severity: AlertSeverity::Regular,
};

const LAST_BATCHED_BLOCK_STUCK: Alert = Alert {
    name: "last_batched_block_stuck",
    title: "Last batched block stuck",
    alert_group: AlertGroup::Batcher,
    expr: formatcp!("changes({}[5m])", LAST_BATCHED_BLOCK.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 1.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1s",
    evaluation_interval_sec: 10,
    severity: AlertSeverity::Regular,
};

pub fn get_apollo_alerts() -> Alerts {
    Alerts::new(&[
        BATCHED_TRANSACTIONS_STUCK,
        CENDE_WRITE_BLOB_FAILURE_ALERT,
        CENDE_WRITE_BLOB_FAILURE_ONCE_ALERT,
        CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY_TOO_HIGH,
        CONSENSUS_BLOCK_NUMBER_STUCK,
        CONSENSUS_BUILD_PROPOSAL_FAILED_ALERT,
        CONSENSUS_BUILD_PROPOSAL_FAILED_ONCE_ALERT,
        CONSENSUS_CONFLICTING_VOTES_RATE,
        CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS_RATIO,
        CONSENSUS_INBOUND_STREAM_EVICTED_ALERT,
        CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR_RATE,
        CONSENSUS_ROUND_ABOVE_ZERO_ALERT,
        CONSENSUS_ROUND_HIGH_AVG,
        CONSENSUS_VALIDATE_PROPOSAL_FAILED_ALERT,
        CONSENSUS_VOTES_NUM_SENT_MESSAGES_ALERT,
        GATEWAY_ADD_TX_LATENCY_INCREASE,
        GATEWAY_ADD_TX_RATE_DROP,
        HTTP_SERVER_IDLE,
        L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY_ALERT,
        L1_GAS_PRICE_REORG_DETECTED_ALERT,
        L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT_ALERT,
        L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT_ALERT,
        L1_MESSAGE_SCRAPER_REORG_DETECTED_ALERT,
        LAST_BATCHED_BLOCK_STUCK,
        MEMPOOL_ADD_TX_RATE_DROP,
        MEMPOOL_GET_TXS_SIZE_DROP,
        MEMPOOL_POOL_SIZE_INCREASE,
        NATIVE_COMPILATION_ERROR_INCREASE,
        STATE_SYNC_LAG,
        STATE_SYNC_STUCK,
    ])
}
