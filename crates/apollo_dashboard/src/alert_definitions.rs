use apollo_consensus::metrics::CONSENSUS_ROUND;
use apollo_gateway::metrics::{GATEWAY_ADD_TX_LATENCY, GATEWAY_TRANSACTIONS_RECEIVED};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_mempool::metrics::{
    MEMPOOL_GET_TXS_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_RECEIVED,
};
use apollo_state_sync_metrics::metrics::{CENTRAL_SYNC_CENTRAL_BLOCK_MARKER, STATE_SYNC_CLASS_MANAGER_MARKER};
use const_format::formatcp;

use crate::dashboard::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    Alerts,
};

pub const DEV_ALERTS_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana_alerts.json";

const GATEWAY_ADD_TX_RATE_DROP: Alert = Alert {
    name: "gateway_add_tx_rate_drop",
    title: "Gateway add_tx rate drop",
    alert_group: AlertGroup::Gateway,
    expr: formatcp!("sum(rate({}[20m])) or vector(0)", GATEWAY_TRANSACTIONS_RECEIVED.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.1,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const GATEWAY_ADD_TX_LATENCY_INCREASE: Alert = Alert {
    name: "gateway_add_tx_latency_increase",
    title: "Gateway avg add_tx latency increase",
    alert_group: AlertGroup::Gateway,
    expr: formatcp!(
        "sum(rate({}_sum[1m]))/sum(rate({}_count[1m]))",
        GATEWAY_ADD_TX_LATENCY.get_name(),
        GATEWAY_ADD_TX_LATENCY.get_name()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 2.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const MEMPOOL_ADD_TX_RATE_DROP: Alert = Alert {
    name: "mempool_add_tx_rate_drop",
    title: "Mempool add_tx rate drop",
    alert_group: AlertGroup::Mempool,
    expr: formatcp!("sum(rate({}[20m])) or vector(0)", MEMPOOL_TRANSACTIONS_RECEIVED.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.1,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const HTTP_SERVER_IDLE: Alert = Alert {
    name: "http_server_idle",
    title: "http server idle",
    alert_group: AlertGroup::HttpServer,
    expr: formatcp!("rate(max({})[60m:])", ADDED_TRANSACTIONS_TOTAL.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.000001,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "5m",
    evaluation_interval_sec: 60,
};

// The rate of add_txs is lower than the rate of transactions inserted into a block since this node
// is not always the proposer.
const MEMPOOL_GET_TXS_SIZE_DROP: Alert = Alert {
    name: "mempool_get_txs_size_drop",
    title: "Mempool get_txs size drop",
    alert_group: AlertGroup::Mempool,
    expr: formatcp!("avg_over_time({}[20m])", MEMPOOL_GET_TXS_SIZE.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.01,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const MEMPOOL_POOL_SIZE_INCREASE: Alert = Alert {
    name: "mempool_pool_size_increase",
    title: "Mempool pool size increase",
    alert_group: AlertGroup::Mempool,
    expr: formatcp!("{}", MEMPOOL_POOL_SIZE.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 2000.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const CONSENSUS_ROUND_HIGH_AVG: Alert = Alert {
    name: "consensus_round_high_avg",
    title: "Consensus round high average",
    alert_group: AlertGroup::Consensus,
    expr: formatcp!("avg_over_time({}[10m])", CONSENSUS_ROUND.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.2,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 20,
};

const STATE_SYNC_LAG: Alert = Alert {
    name: "state_sync_lag",
    title: "State sync lag",
    alert_group: AlertGroup::Sync,
    expr: formatcp!(
        "{} - {}",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name(),
        STATE_SYNC_CLASS_MANAGER_MARKER.get_name()
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 5.0, // Alert if lag is more than 5 blocks
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 60,
};

const STATE_SYNC_STUCK: Alert = Alert {
    name: "state_sync_stuck",
    title: "State sync stuck",
    alert_group: AlertGroup::Sync,
    expr: formatcp!(
        "{} == {}",
        STATE_SYNC_CLASS_MANAGER_MARKER.get_name(),
        formatcp!("{} offset 1m", STATE_SYNC_CLASS_MANAGER_MARKER.get_name())
    ),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::GreaterThan,
        comparison_value: 0.0,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "1m",
    evaluation_interval_sec: 60,
};

pub const SEQUENCER_ALERTS: Alerts = Alerts::new(&[
    GATEWAY_ADD_TX_RATE_DROP,
    GATEWAY_ADD_TX_LATENCY_INCREASE,
    MEMPOOL_ADD_TX_RATE_DROP,
    MEMPOOL_GET_TXS_SIZE_DROP,
    HTTP_SERVER_IDLE,
    MEMPOOL_POOL_SIZE_INCREASE,
    CONSENSUS_ROUND_HIGH_AVG,
    STATE_SYNC_LAG,
    STATE_SYNC_STUCK,
]);
