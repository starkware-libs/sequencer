use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND};
use apollo_gateway::metrics::{GATEWAY_ADD_TX_LATENCY, GATEWAY_TRANSACTIONS_RECEIVED};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_mempool::metrics::{
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_RECEIVED,
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

pub const DEV_ALERTS_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana_alerts.json";

// Within 30s the metrics should be updated at least twice.
// If in one of those times the block number is not updated, fire an alert.
const CONSENSUS_BLOCK_NUMBER_STUCK: Alert = Alert {
    name: "consensus_block_number_stuck",
    title: "Consensus block number stuck",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("changes({}[30s])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 2.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1s",
    evaluation_interval_sec: 10,
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

// The rate of add_txs is lower than the rate of transactions inserted into a block since this node
// is not always the proposer.
const MEMPOOL_GET_TXS_SIZE_DROP: Alert = Alert {
    name: "mempool_get_txs_size_drop",
    title: "Mempool get_txs size drop",
    alert_group: AlertGroup::Mempool,
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

pub const SEQUENCER_ALERTS: Alerts = Alerts::new(&[
    CONSENSUS_BLOCK_NUMBER_STUCK,
    GATEWAY_ADD_TX_RATE_DROP,
    GATEWAY_ADD_TX_LATENCY_INCREASE,
    MEMPOOL_ADD_TX_RATE_DROP,
    MEMPOOL_GET_TXS_SIZE_DROP,
    HTTP_SERVER_IDLE,
    MEMPOOL_POOL_SIZE_INCREASE,
    CONSENSUS_ROUND_HIGH_AVG,
    NATIVE_COMPILATION_ERROR_INCREASE,
]);
