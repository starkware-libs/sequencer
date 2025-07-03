use std::collections::HashSet;

use apollo_batcher::metrics::{BATCHED_TRANSACTIONS, PRECONFIRMED_BLOCK_WRITTEN};
use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_INBOUND_STREAM_EVICTED,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_ROUND,
};
use apollo_consensus_manager::metrics::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use apollo_consensus_orchestrator::metrics::{
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR,
};
use apollo_gateway::metrics::GATEWAY_TRANSACTIONS_RECEIVED;
use apollo_http_server::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
    HTTP_SERVER_ADD_TX_LATENCY,
};
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};
use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};
use apollo_mempool::metrics::{
    MEMPOOL_EVICTIONS_COUNT,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_TRANSACTIONS_DROPPED,
    MEMPOOL_TRANSACTIONS_RECEIVED,
};
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_NUM_CONNECTED_PEERS;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
};
use blockifier::metrics::NATIVE_COMPILATION_ERROR;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    Alerts,
};

pub const DEV_ALERTS_JSON_PATH: &str = "crates/apollo_dashboard/resources/dev_grafana_alerts.json";

const PENDING_DURATION_DEFAULT: &str = "30s";
const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;

fn get_consensus_block_number_stuck() -> Alert {
    Alert {
        name: "consensus_block_number_stuck",
        title: "Consensus block number stuck",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[5m])", CONSENSUS_BLOCK_NUMBER.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

// If this happens, we expect to also see other nodes alert on `consensus_validate_proposal_failed`.
fn get_consensus_build_proposal_failed_alert() -> Alert {
    Alert {
        name: "consensus_build_proposal_failed",
        title: "Consensus build proposal failed",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_consensus_build_proposal_failed_once_alert() -> Alert {
    Alert {
        name: "consensus_build_proposal_failed_once",
        title: "Consensus build proposal failed once",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_consensus_validate_proposal_failed_alert() -> Alert {
    Alert {
        name: "consensus_validate_proposal_failed",
        title: "Consensus validate proposal failed",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CONSENSUS_PROPOSALS_INVALID.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_consensus_decisions_reached_by_consensus_ratio() -> Alert {
    Alert {
        name: "consensus_decisions_reached_by_consensus_ratio",
        title: "Consensus decisions reached by consensus ratio",
        alert_group: AlertGroup::Consensus,
        // Clamp to avoid divide by 0.
        expr: format!(
            "increase({consensus}[10m]) / clamp_min(increase({sync}[10m]) + \
             increase({consensus}[10m]), 1)",
            consensus = CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter(),
            sync = CONSENSUS_DECISIONS_REACHED_BY_SYNC.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_consensus_inbound_stream_evicted_alert() -> Alert {
    Alert {
        name: "consensus_inbound_stream_evicted",
        title: "Consensus inbound stream evicted",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CONSENSUS_INBOUND_STREAM_EVICTED.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_consensus_votes_num_sent_messages_alert() -> Alert {
    Alert {
        name: "consensus_votes_num_sent_messages",
        title: "Consensus votes num sent messages",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "increase({}[20m])",
            CONSENSUS_VOTES_NUM_SENT_MESSAGES.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 20.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_cende_write_prev_height_blob_latency_too_high() -> Alert {
    Alert {
        name: "cende_write_prev_height_blob_latency_too_high",
        title: "Cende write prev height blob latency too high",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "avg_over_time({}[20m])",
            CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            // This is 50% of the proposal timeout.
            comparison_value: 1.5,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_cende_write_blob_failure_alert() -> Alert {
    Alert {
        name: "cende_write_blob_failure",
        title: "Cende write blob failure",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_cende_write_blob_failure_once_alert() -> Alert {
    Alert {
        name: "cende_write_blob_failure_once",
        title: "Cende write blob failure once",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_consensus_l1_gas_price_provider_failure() -> Alert {
    Alert {
        name: "consensus_l1_gas_price_provider_failure",
        title: "Consensus L1 gas price provider failure",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "increase({}[1h])",
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_consensus_l1_gas_price_provider_failure_once() -> Alert {
    Alert {
        name: "consensus_l1_gas_price_provider_failure_once",
        title: "Consensus L1 gas price provider failure once",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "increase({}[1h])",
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_consensus_round_above_zero() -> Alert {
    Alert {
        name: "consensus_round_above_zero",
        title: "Consensus round above zero",
        alert_group: AlertGroup::Consensus,
        expr: format!("max_over_time({}[1h])", CONSENSUS_ROUND.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_consensus_conflicting_votes() -> Alert {
    Alert {
        name: "consensus_conflicting_votes",
        title: "Consensus conflicting votes",
        alert_group: AlertGroup::Consensus,
        expr: format!("increase({}[20m])", CONSENSUS_CONFLICTING_VOTES.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        // TODO(matan): Increase severity once slashing is supported.
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_gateway_add_tx_idle() -> Alert {
    Alert {
        name: "gateway_add_tx_idle",
        title: "Gateway add_tx idle",
        alert_group: AlertGroup::Gateway,
        expr: format!(
            "increase({}[20m]) or vector(0)",
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

// TODO(shahak): add gateway latency alert

fn get_mempool_add_tx_idle() -> Alert {
    Alert {
        name: "mempool_add_tx_idle",
        title: "Mempool add_tx idle",
        alert_group: AlertGroup::Mempool,
        expr: format!(
            "increase({}[20m]) or vector(0)",
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_http_server_add_tx_idle() -> Alert {
    Alert {
        name: "http_server_add_tx_idle",
        title: "HTTP Server add_tx idle",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[20m]) or vector(0)",
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_http_server_idle() -> Alert {
    Alert {
        name: "http_server_idle",
        title: "http server idle",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[20m]) or vector(0)",
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_http_server_internal_error_ratio() -> Alert {
    Alert {
        name: "http_server_internal_error_ratio",
        title: "http server internal error ratio",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.2,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_http_server_internal_error_once() -> Alert {
    Alert {
        name: "http_server_internal_error_once",
        title: "http server internal error once",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[20m]) or vector(0)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_eth_to_strk_error_count_alert() -> Alert {
    Alert {
        name: "eth_to_strk_error_count",
        title: "Eth to Strk error count",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!("increase({}[1h])", ETH_TO_STRK_ERROR_COUNT.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: "1m",
        evaluation_interval_sec: 20,
        severity: AlertSeverity::Informational,
    }
}

fn get_eth_to_strk_success_count_alert() -> Alert {
    Alert {
        name: "eth_to_strk_success_count",
        title: "Eth to Strk success count",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!("increase({}[1h])", ETH_TO_STRK_SUCCESS_COUNT.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_l1_gas_price_scraper_success_count_alert() -> Alert {
    Alert {
        name: "l1_gas_price_scraper_success_count",
        title: "L1 gas price scraper success count",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!(
            "increase({}[1h])",
            L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_http_server_no_successful_transactions() -> Alert {
    Alert {
        name: "http_server_no_successful_transactions",
        title: "http server no successful transactions",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[1h]) or vector(0)",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_http_server_low_successful_transactions_rate() -> Alert {
    Alert {
        name: "http_server_low_successful_transactions_rate",
        title: "http server low successful transactions rate",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "rate({}[5m]) or vector(0)",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_http_server_high_transaction_failure_ratio() -> Alert {
    Alert {
        name: "http_server_high_transaction_failure_ratio",
        title: "http server high transaction failure ratio",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_FAILURE.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

/// Triggers if the average latency of `add_tx` calls, across all HTTP servers, exceeds 2 seconds
/// over a 5-minute window.
fn get_http_server_avg_add_tx_latency_alert() -> Alert {
    let sum_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_sum_with_filter();
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();

    Alert {
        name: "http_server_avg_add_tx_latency",
        title: "High HTTP server average add_tx latency",
        alert_group: AlertGroup::HttpServer,
        expr: format!("rate({sum_metric}[5m]) / rate({count_metric}[5m])"),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

/// Triggers when the slowest 5% of transactions for a specific HTTP server are taking longer than 2
/// seconds over a 5-minute window.
fn get_http_server_p95_add_tx_latency_alert() -> Alert {
    Alert {
        name: "http_server_p95_add_tx_latency",
        title: "High HTTP server P95 add_tx latency",
        alert_group: AlertGroup::HttpServer,
        expr: format!(
            "histogram_quantile(0.95, sum(rate({}[5m])) by (le))",
            HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_l1_gas_price_scraper_baselayer_error_count_alert() -> Alert {
    Alert {
        name: "l1_gas_price_scraper_baselayer_error_count",
        title: "L1 gas price scraper baselayer error count",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!(
            "increase({}[5m])",
            L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_l1_gas_price_provider_insufficient_history_alert() -> Alert {
    Alert {
        name: "l1_gas_price_provider_insufficient_history",
        title: "L1 gas price provider insufficient history",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!(
            "increase({}[1m])",
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_l1_gas_price_reorg_detected_alert() -> Alert {
    Alert {
        name: "l1_gas_price_scraper_reorg_detected",
        title: "L1 gas price scraper reorg detected",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!(
            "increase({}[1m])",
            L1_GAS_PRICE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_l1_message_scraper_no_successes_alert() -> Alert {
    Alert {
        name: "l1_message_no_successes",
        title: "L1 message no successes",
        alert_group: AlertGroup::L1GasPrice,
        expr: format!("increase({}[20m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_l1_message_scraper_baselayer_error_count_alert() -> Alert {
    Alert {
        name: "l1_message_scraper_baselayer_error_count",
        title: "L1 message scraper baselayer error count",
        alert_group: AlertGroup::L1Messages,
        expr: format!(
            "increase({}[1h])",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_l1_message_scraper_reorg_detected_alert() -> Alert {
    Alert {
        name: "l1_message_scraper_reorg_detected",
        title: "L1 message scraper reorg detected",
        alert_group: AlertGroup::L1Messages,
        expr: format!(
            "increase({}[1m])",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_mempool_pool_size_increase() -> Alert {
    Alert {
        name: "mempool_pool_size_increase",
        title: "Mempool pool size increase",
        alert_group: AlertGroup::Mempool,
        expr: MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2000.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_mempool_transaction_drop_ratio() -> Alert {
    Alert {
        name: "mempool_transaction_drop_ratio",
        title: "Mempool transaction drop ratio",
        alert_group: AlertGroup::Mempool,
        expr: format!(
            "increase({}[10m]) / clamp_min(increase({}[10m]), 1)",
            MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter(),
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_consensus_round_high() -> Alert {
    Alert {
        name: "consensus_round_high",
        title: "Consensus round high",
        alert_group: AlertGroup::Consensus,
        expr: format!("max_over_time({}[1m])", CONSENSUS_ROUND.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 20.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_consensus_round_above_zero_ratio() -> Alert {
    Alert {
        name: "consensus_round_above_zero_ratio",
        title: "Consensus round above zero ratio",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "count_over_time(({metric} > 0)[1h]) / count_over_time({metric}[1h])",
            metric = CONSENSUS_ROUND.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.05,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: 10,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_native_compilation_error_increase() -> Alert {
    Alert {
        name: "native_compilation_error",
        title: "Native compilation alert",
        alert_group: AlertGroup::Batcher,
        expr: format!("increase({}[1h])", NATIVE_COMPILATION_ERROR.get_name()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Informational,
    }
}

fn get_state_sync_lag() -> Alert {
    Alert {
        name: "state_sync_lag",
        title: "State sync lag",
        alert_group: AlertGroup::StateSync,
        expr: format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ), // Alert when the central sync is ahead of the class manager by more than 5 blocks
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_state_sync_stuck() -> Alert {
    Alert {
        name: "state_sync_stuck",
        title: "State sync stuck",
        alert_group: AlertGroup::StateSync,
        expr: format!("increase({}[5m])", STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()), /* Alert is triggered when the class manager marker is not updated for 5m */
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_batched_transactions_stuck() -> Alert {
    Alert {
        name: "batched_transactions_stuck",
        title: "Batched transactions stuck",
        alert_group: AlertGroup::Batcher,
        expr: format!("changes({}[5m])", BATCHED_TRANSACTIONS.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

fn get_preconfirmed_block_not_written() -> Alert {
    Alert {
        name: "preconfirmed_block_not_written",
        title: "Preconfirmed block not written",
        alert_group: AlertGroup::Batcher,
        expr: format!("increase({}[1h])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_consensus_p2p_peer_down() -> Alert {
    Alert {
        name: "consensus_p2p_peer_down",
        title: "Consensus p2p peer down",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "max_over_time({}[1h])",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

fn get_consensus_p2p_not_enough_peers_for_quorum() -> Alert {
    Alert {
        name: "consensus_p2p_not_enough_peers_for_quorum",
        title: "Consensus p2p not enough peers for quorum",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            "max_over_time({}[5m])",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators and
            // assume_no_malicious_validators
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

/// Alert if there were too many disconnections in the given timespan
fn get_consensus_p2p_disconnections() -> Alert {
    Alert {
        name: "consensus_p2p_disconnections",
        title: "Consensus p2p disconnections",
        alert_group: AlertGroup::Consensus,
        expr: format!(
            // TODO(shahak): find a way to make this depend on num_validators
            // Dividing by two since this counts both disconnections and reconnections
            "changes({}[1h]) / 2",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn get_mempool_p2p_peer_down() -> Alert {
    Alert {
        name: "mempool_p2p_peer_down",
        title: "Mempool p2p peer down",
        alert_group: AlertGroup::Mempool,
        expr: format!(
            "max_over_time({}[1h])",
            MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
    }
}

/// Alert if there were too many disconnections in the given timespan
fn get_mempool_p2p_disconnections() -> Alert {
    Alert {
        name: "mempool_p2p_disconnections",
        title: "Mempool p2p disconnections",
        alert_group: AlertGroup::Mempool,
        expr: format!(
            // TODO(shahak): find a way to make this depend on num_validators
            // Dividing by two since this counts both disconnections and reconnections
            "changes({}[1h]) / 2",
            MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::WorkingHours,
    }
}

fn verify_unique_names(alerts: &[Alert]) {
    let mut names = HashSet::new();
    for alert in alerts.iter() {
        if !names.insert(&alert.name) {
            panic!("Duplicate alert name found: {}", alert.name);
        }
    }
}

fn get_mempool_evictions_count_alert() -> Alert {
    Alert {
        name: "mempool_evictions_count",
        title: "Mempool evictions count",
        alert_group: AlertGroup::Mempool,
        expr: MEMPOOL_EVICTIONS_COUNT.get_name_with_filter().to_string(),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
    }
}

pub fn get_apollo_alerts() -> Alerts {
    let alerts = vec![
        get_batched_transactions_stuck(),
        get_cende_write_blob_failure_alert(),
        get_cende_write_blob_failure_once_alert(),
        get_cende_write_prev_height_blob_latency_too_high(),
        get_consensus_block_number_stuck(),
        get_consensus_build_proposal_failed_alert(),
        get_consensus_build_proposal_failed_once_alert(),
        get_consensus_conflicting_votes(),
        get_consensus_decisions_reached_by_consensus_ratio(),
        get_consensus_inbound_stream_evicted_alert(),
        get_consensus_l1_gas_price_provider_failure(),
        get_consensus_l1_gas_price_provider_failure_once(),
        get_consensus_p2p_disconnections(),
        get_consensus_p2p_not_enough_peers_for_quorum(),
        get_consensus_p2p_peer_down(),
        get_consensus_round_above_zero(),
        get_consensus_round_above_zero_ratio(),
        get_consensus_round_high(),
        get_consensus_validate_proposal_failed_alert(),
        get_consensus_votes_num_sent_messages_alert(),
        get_gateway_add_tx_idle(),
        get_http_server_idle(),
        get_http_server_add_tx_idle(),
        get_http_server_high_transaction_failure_ratio(),
        get_http_server_internal_error_ratio(),
        get_http_server_internal_error_once(),
        get_http_server_no_successful_transactions(),
        get_http_server_avg_add_tx_latency_alert(),
        get_http_server_p95_add_tx_latency_alert(),
        get_l1_gas_price_provider_insufficient_history_alert(),
        get_l1_gas_price_reorg_detected_alert(),
        get_l1_gas_price_scraper_success_count_alert(),
        get_l1_gas_price_scraper_baselayer_error_count_alert(),
        get_eth_to_strk_error_count_alert(),
        get_eth_to_strk_success_count_alert(),
        get_l1_message_scraper_no_successes_alert(),
        get_l1_message_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_reorg_detected_alert(),
        get_mempool_add_tx_idle(),
        get_mempool_evictions_count_alert(),
        get_mempool_p2p_disconnections(),
        get_mempool_p2p_peer_down(),
        get_mempool_pool_size_increase(),
        get_mempool_transaction_drop_ratio(),
        get_native_compilation_error_increase(),
        get_preconfirmed_block_not_written(),
        get_state_sync_lag(),
        get_state_sync_stuck(),
    ];
    verify_unique_names(&alerts);
    Alerts::new(alerts)
}
