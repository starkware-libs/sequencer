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
    CONSENSUS_ROUND_ABOVE_ZERO,
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
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
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
use apollo_metrics::metric_label_filter;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
};
use blockifier::metrics::NATIVE_COMPILATION_ERROR;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    Alerts,
};

const PENDING_DURATION_DEFAULT: &str = "30s";
const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;

pub fn get_dev_alerts_json_path(alert_env_filtering: AlertEnvFiltering) -> String {
    format!("crates/apollo_dashboard/resources/dev_grafana_alerts_{}.json", alert_env_filtering)
}

fn get_consensus_block_number_stuck() -> Alert {
    Alert::new(
        "consensus_block_number_stuck",
        "Consensus block number stuck",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[5m])) or vector(0)",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

// If this happens, we expect to also see other nodes alert on `consensus_validate_proposal_failed`.
fn get_consensus_build_proposal_failed_alert() -> Alert {
    Alert::new(
        "consensus_build_proposal_failed",
        "Consensus build proposal failed",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_build_proposal_failed_once_alert() -> Alert {
    Alert::new(
        "consensus_build_proposal_failed_once",
        "Consensus build proposal failed once",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_BUILD_PROPOSAL_FAILED.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_validate_proposal_failed_alert() -> Alert {
    Alert::new(
        "consensus_validate_proposal_failed",
        "Consensus validate proposal failed",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_PROPOSALS_INVALID.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_decisions_reached_by_consensus_ratio() -> Alert {
    Alert::new(
        "consensus_decisions_reached_by_consensus_ratio",
        "Consensus decisions reached by consensus ratio",
        AlertGroup::Consensus,
        // Clamp to avoid divide by 0.
        format!(
            "increase({consensus}[10m]) / clamp_min(increase({sync}[10m]) + \
             increase({consensus}[10m]), 1)",
            consensus = CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter(),
            sync = CONSENSUS_DECISIONS_REACHED_BY_SYNC.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_inbound_stream_evicted_alert() -> Alert {
    Alert::new(
        "consensus_inbound_stream_evicted",
        "Consensus inbound stream evicted",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_INBOUND_STREAM_EVICTED.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_votes_num_sent_messages_alert() -> Alert {
    Alert::new(
        "consensus_votes_num_sent_messages",
        "Consensus votes num sent messages",
        AlertGroup::Consensus,
        format!("increase({}[20m])", CONSENSUS_VOTES_NUM_SENT_MESSAGES.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 20.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_cende_write_prev_height_blob_latency_too_high() -> Alert {
    Alert::new(
        "cende_write_prev_height_blob_latency_too_high",
        "Cende write prev height blob latency too high",
        AlertGroup::Consensus,
        format!(
            "rate({}[20m]) / clamp_min(rate({}[20m]), 0.0000001)",
            CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_sum_with_filter(),
            CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_count_with_filter(),
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            // This is 50% of the proposal timeout.
            comparison_value: 1.5,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_cende_write_blob_failure_alert() -> Alert {
    Alert::new(
        "cende_write_blob_failure",
        "Cende write blob failure",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_cende_write_blob_failure_once_alert() -> Alert {
    Alert::new(
        "cende_write_blob_failure_once",
        "Cende write blob failure once",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_l1_gas_price_provider_failure() -> Alert {
    Alert::new(
        "consensus_l1_gas_price_provider_failure",
        "Consensus L1 gas price provider failure",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_l1_gas_price_provider_failure_once() -> Alert {
    Alert::new(
        "consensus_l1_gas_price_provider_failure_once",
        "Consensus L1 gas price provider failure once",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_round_above_zero() -> Alert {
    Alert::new(
        "consensus_round_above_zero",
        "Consensus round above zero",
        AlertGroup::Consensus,
        format!("max_over_time({}[1h])", CONSENSUS_ROUND.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_conflicting_votes() -> Alert {
    Alert::new(
        "consensus_conflicting_votes",
        "Consensus conflicting votes",
        AlertGroup::Consensus,
        format!("increase({}[20m])", CONSENSUS_CONFLICTING_VOTES.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        // TODO(matan): Increase severity once slashing is supported.
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_gateway_add_tx_idle() -> Alert {
    Alert::new(
        "gateway_add_tx_idle",
        "Gateway add_tx idle",
        AlertGroup::Gateway,
        format!(
            "sum(increase({}[20m])) or vector(0)",
            GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

// TODO(shahak): add gateway latency alert

fn get_mempool_add_tx_idle() -> Alert {
    Alert::new(
        "mempool_add_tx_idle",
        "Mempool add_tx idle",
        AlertGroup::Mempool,
        format!(
            "sum(increase({}[20m])) or vector(0)",
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_add_tx_idle() -> Alert {
    Alert::new(
        "http_server_add_tx_idle",
        "HTTP Server add_tx idle",
        AlertGroup::HttpServer,
        format!(
            "sum(increase({}[20m])) or vector(0)",
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_internal_error_ratio() -> Alert {
    Alert::new(
        "http_server_internal_error_ratio",
        "http server internal error ratio",
        AlertGroup::HttpServer,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.2,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_internal_error_once() -> Alert {
    Alert::new(
        "http_server_internal_error_once",
        "http server internal error once",
        AlertGroup::HttpServer,
        format!(
            "increase({}[20m]) or vector(0)",
            ADDED_TRANSACTIONS_INTERNAL_ERROR.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_eth_to_strk_error_count_alert() -> Alert {
    Alert::new(
        "eth_to_strk_error_count",
        "Eth to Strk error count",
        AlertGroup::L1GasPrice,
        format!("increase({}[1h])", ETH_TO_STRK_ERROR_COUNT.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        "1m",
        20,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_eth_to_strk_success_count_alert() -> Alert {
    Alert::new(
        "eth_to_strk_success_count",
        "Eth to Strk success count",
        AlertGroup::L1GasPrice,
        format!("increase({}[1h])", ETH_TO_STRK_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_l1_gas_price_scraper_success_count_alert() -> Alert {
    Alert::new(
        "l1_gas_price_scraper_success_count",
        "L1 gas price scraper success count",
        AlertGroup::L1GasPrice,
        format!("increase({}[1h])", L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_no_successful_transactions() -> Alert {
    Alert::new(
        "http_server_no_successful_transactions",
        "http server no successful transactions",
        AlertGroup::HttpServer,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_low_successful_transaction_rate() -> Alert {
    Alert::new(
        "http_server_low_successful_transaction_rate",
        "http server low successful transaction rate",
        AlertGroup::HttpServer,
        format!("rate({}[5m]) or vector(0)", ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.01,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_high_transaction_failure_ratio() -> Alert {
    Alert::new(
        "http_server_high_transaction_failure_ratio",
        "http server high transaction failure ratio",
        AlertGroup::HttpServer,
        format!(
            "(increase({}[1h]) - increase({}[1h])) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_FAILURE.get_name_with_filter(),
            ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

/// Triggers if the average latency of `add_tx` calls, across all HTTP servers, exceeds 2 seconds
/// over a 5-minute window.
fn get_http_server_avg_add_tx_latency_alert() -> Alert {
    let sum_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_sum_with_filter();
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();

    Alert::new(
        "http_server_avg_add_tx_latency",
        "High HTTP server average add_tx latency",
        AlertGroup::HttpServer,
        format!("rate({sum_metric}[5m]) / rate({count_metric}[5m])"),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

/// Triggers when the slowest 5% of transactions for a specific HTTP server are taking longer than 2
/// seconds over a 5-minute window.
fn get_http_server_p95_add_tx_latency_alert() -> Alert {
    Alert::new(
        "http_server_p95_add_tx_latency",
        "High HTTP server P95 add_tx latency",
        AlertGroup::HttpServer,
        format!(
            "histogram_quantile(0.95, sum(rate({}[5m])) by (le))",
            HTTP_SERVER_ADD_TX_LATENCY.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_l1_gas_price_scraper_baselayer_error_count_alert() -> Alert {
    Alert::new(
        "l1_gas_price_scraper_baselayer_error_count",
        "L1 gas price scraper baselayer error count",
        AlertGroup::L1GasPrice,
        format!(
            "increase({}[5m])",
            L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_l1_gas_price_provider_insufficient_history_alert() -> Alert {
    Alert::new(
        "l1_gas_price_provider_insufficient_history",
        "L1 gas price provider insufficient history",
        AlertGroup::L1GasPrice,
        format!(
            "increase({}[1m])",
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_l1_gas_price_reorg_detected_alert() -> Alert {
    Alert::new(
        "l1_gas_price_scraper_reorg_detected",
        "L1 gas price scraper reorg detected",
        AlertGroup::L1GasPrice,
        format!("increase({}[1m])", L1_GAS_PRICE_SCRAPER_REORG_DETECTED.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_l1_message_scraper_no_successes_alert() -> Alert {
    Alert::new(
        "l1_message_no_successes",
        "L1 message no successes",
        AlertGroup::L1GasPrice,
        format!("increase({}[20m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_l1_message_scraper_baselayer_error_count_alert() -> Alert {
    Alert::new(
        "l1_message_scraper_baselayer_error_count",
        "L1 message scraper baselayer error count",
        AlertGroup::L1Messages,
        format!(
            "increase({}[1h])",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_l1_message_scraper_reorg_detected_alert() -> Alert {
    Alert::new(
        "l1_message_scraper_reorg_detected",
        "L1 message scraper reorg detected",
        AlertGroup::L1Messages,
        format!(
            "increase({}[1m])",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_mempool_pool_size_increase() -> Alert {
    Alert::new(
        "mempool_pool_size_increase",
        "Mempool pool size increase",
        AlertGroup::Mempool,
        MEMPOOL_POOL_SIZE.get_name_with_filter().to_string(),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2000.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_mempool_transaction_drop_ratio() -> Alert {
    Alert::new(
        "mempool_transaction_drop_ratio",
        "Mempool transaction drop ratio",
        AlertGroup::Mempool,
        format!(
            "increase({}[10m]) / clamp_min(increase({}[10m]), 1)",
            MEMPOOL_TRANSACTIONS_DROPPED.get_name_with_filter(),
            MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.5,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_round_high() -> Alert {
    Alert::new(
        "consensus_round_high",
        "Consensus round high",
        AlertGroup::Consensus,
        format!("max_over_time({}[1m])", CONSENSUS_ROUND.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 20.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_round_above_zero_ratio() -> Alert {
    Alert::new(
        "consensus_round_above_zero_ratio",
        "Consensus round above zero ratio",
        AlertGroup::Consensus,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter(),
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.05,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        10,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_native_compilation_error_increase() -> Alert {
    Alert::new(
        "native_compilation_error",
        "Native compilation alert",
        AlertGroup::Batcher,
        format!("increase({}[1h])", NATIVE_COMPILATION_ERROR.get_name()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

fn get_state_sync_lag() -> Alert {
    Alert::new(
        "state_sync_lag",
        "State sync lag",
        AlertGroup::StateSync,
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ), // Alert when the central sync is ahead of the class manager by more than 5 blocks
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_state_sync_stuck() -> Alert {
    Alert::new(
        "state_sync_stuck",
        "State sync stuck",
        AlertGroup::StateSync,
        format!("increase({}[5m])", STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()), /* Alert is triggered when the class manager marker is not updated for 5m */
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_batched_transactions_stuck() -> Alert {
    Alert::new(
        "batched_transactions_stuck",
        "Batched transactions stuck",
        AlertGroup::Batcher,
        format!("changes({}[5m])", BATCHED_TRANSACTIONS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_preconfirmed_block_not_written() -> Alert {
    Alert::new(
        "preconfirmed_block_not_written",
        "Preconfirmed block not written",
        AlertGroup::Batcher,
        format!("increase({}[1h])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_p2p_peer_down() -> Alert {
    Alert::new(
        "consensus_p2p_peer_down",
        "Consensus p2p peer down",
        AlertGroup::Consensus,
        format!("max_over_time({}[1h])", CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

fn get_consensus_p2p_not_enough_peers_for_quorum() -> Alert {
    Alert::new(
        "consensus_p2p_not_enough_peers_for_quorum",
        "Consensus p2p not enough peers for quorum",
        AlertGroup::Consensus,
        format!("max_over_time({}[5m])", CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators and
            // assume_no_malicious_validators
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

/// Alert if there were too many disconnections in the given timespan
fn get_consensus_p2p_disconnections() -> Alert {
    Alert::new(
        "consensus_p2p_disconnections",
        "Consensus p2p disconnections",
        AlertGroup::Consensus,
        format!(
            // TODO(shahak): find a way to make this depend on num_validators
            // Dividing by two since this counts both disconnections and reconnections
            "changes({}[1h]) / 2",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_mempool_p2p_peer_down() -> Alert {
    Alert::new(
        "mempool_p2p_peer_down",
        "Mempool p2p peer down",
        AlertGroup::Mempool,
        format!("max_over_time({}[1h])", MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        AlertEnvFiltering::All,
    )
}

/// Alert if there were too many disconnections in the given timespan
fn get_mempool_p2p_disconnections() -> Alert {
    Alert::new(
        "mempool_p2p_disconnections",
        "Mempool p2p disconnections",
        AlertGroup::Mempool,
        format!(
            // TODO(shahak): find a way to make this depend on num_validators
            // Dividing by two since this counts both disconnections and reconnections
            "changes({}[1h]) / 2",
            MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
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
    Alert::new(
        "mempool_evictions_count",
        "Mempool evictions count",
        AlertGroup::Mempool,
        MEMPOOL_EVICTIONS_COUNT.get_name_with_filter().to_string(),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_state_not_ready_alert() -> Alert {
    Alert {
        name: "pod_state_not_ready",
        title: "Pod State Not Ready",
        alert_group: AlertGroup::General,
        expr: format!("kube_pod_container_status_ready{}", metric_label_filter!()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_state_crashloopbackoff() -> Alert {
    Alert {
        name: "pod_state_crashloopbackoff",
        title: "Pod State CrashLoopBackOff",
        alert_group: AlertGroup::General,
        expr: format!(
            // Format the main query and append `reason="CrashLoopBackOff"` inside the label set
            // Using absent trick to convert "NoData" to 0
            "sum by(container, pod, namespace) \
             (kube_pod_container_status_waiting_reason{}{label}) or \
             absent(kube_pod_container_status_waiting_reason{}{label}) * 0",
            &metric_label_filter!()[..metric_label_filter!().len() - 1],
            &metric_label_filter!()[..metric_label_filter!().len() - 1],
            label = ", reason=\"CrashLoopBackOff\"}"
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_high_memory_utilization() -> Alert {
    Alert {
        name: "pod_state_high_memory_utilization",
        title: "Pod High Memory Utilization ( >70% )",
        alert_group: AlertGroup::General,
        expr: format!(
            "max(container_memory_working_set_bytes{0}) by (container, pod, namespace) / \
             max(container_spec_memory_limit_bytes{0}) by (container, pod, namespace) * 100",
            metric_label_filter!()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 70.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_critical_memory_utilization() -> Alert {
    Alert {
        name: "pod_critical_memory_utilization",
        title: "Pod Critical Memory Utilization ( >85% )",
        alert_group: AlertGroup::General,
        expr: format!(
            "max(container_memory_working_set_bytes{0}) by (container, pod, namespace) / \
             max(container_spec_memory_limit_bytes{0}) by (container, pod, namespace) * 100",
            metric_label_filter!()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 85.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_high_cpu_utilization() -> Alert {
    Alert {
        name: "pod_high_cpu_utilization",
        title: "Pod High CPU Utilization ( >90% )",
        alert_group: AlertGroup::General,
        expr: format!(
            "max(irate(container_cpu_usage_seconds_total{0}[2m])) by (container, pod, namespace) \
             / (max(container_spec_cpu_quota{0}/100000) by (container, pod, namespace)) * 100",
            metric_label_filter!()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 90.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_high_disk_utilization() -> Alert {
    Alert {
        name: "pod_high_disk_utilization",
        title: "Pod High Disk Utilization ( >70% )",
        alert_group: AlertGroup::General,
        expr: format!(
            "max by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}) / (min \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_available_bytes{0}) + max \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}))*100",
            metric_label_filter!()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 70.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::DayOnly,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

fn get_general_pod_critical_disk_utilization() -> Alert {
    Alert {
        name: "pod_critical_disk_utilization",
        title: "Pod Critical Disk Utilization ( >90% )",
        alert_group: AlertGroup::General,
        expr: format!(
            "max by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}) / (min \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_available_bytes{0}) + max \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}))*100",
            metric_label_filter!()
        ),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 90.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
        alert_env_filtering: AlertEnvFiltering::All,
    }
}

pub fn get_apollo_alerts(alert_env_filtering: AlertEnvFiltering) -> Alerts {
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
        get_eth_to_strk_error_count_alert(),
        get_eth_to_strk_success_count_alert(),
        get_gateway_add_tx_idle(),
        get_general_pod_state_not_ready_alert(),
        get_general_pod_state_crashloopbackoff(),
        get_general_pod_high_memory_utilization(),
        get_general_pod_critical_memory_utilization(),
        get_general_pod_high_cpu_utilization(),
        get_general_pod_high_disk_utilization(),
        get_general_pod_critical_disk_utilization(),
        get_http_server_add_tx_idle(),
        get_http_server_avg_add_tx_latency_alert(),
        get_http_server_high_transaction_failure_ratio(),
        get_http_server_internal_error_ratio(),
        get_http_server_internal_error_once(),
        get_http_server_low_successful_transaction_rate(),
        get_http_server_no_successful_transactions(),
        get_http_server_p95_add_tx_latency_alert(),
        get_l1_gas_price_provider_insufficient_history_alert(),
        get_l1_gas_price_reorg_detected_alert(),
        get_l1_gas_price_scraper_success_count_alert(),
        get_l1_gas_price_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_no_successes_alert(),
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
    Alerts::new(alerts, alert_env_filtering)
}
