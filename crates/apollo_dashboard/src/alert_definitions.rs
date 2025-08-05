use std::time::Duration;

use apollo_batcher::metrics::{BATCHED_TRANSACTIONS, PRECONFIRMED_BLOCK_WRITTEN};
use apollo_consensus::metrics::{
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_INBOUND_STREAM_EVICTED,
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
const SECS_IN_MIN: u64 = 60;

pub fn get_dev_alerts_json_path(alert_env_filtering: AlertEnvFiltering) -> String {
    format!("crates/apollo_dashboard/resources/dev_grafana_alerts_{}.json", alert_env_filtering)
}

// TODO(guy.f): Can we have spaces in the alert names? If so, do we want to make the alert name and
// title the same?

/// Block number is stuck for more than duration minutes.
// TODO(shahak): Remove this for mainnet when we can.
fn get_consensus_block_number_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name,
        "Consensus block number stuck",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[{}s])) or vector(0)",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter(),
            duration.as_secs(),
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_block_number_stuck_vec() -> Vec<Alert> {
    vec![
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Sos,
        ),
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::DayOnly,
        ),
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}

/// Block number progressed slowly (< 10) in the last 5 minutes.
fn get_consensus_block_number_progress_is_slow(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "get_consensus_block_number_progress_is_slow",
        "Consensus block number progress is slow",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[2m])) or vector(0)",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 25.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_block_number_progress_is_slow_vec() -> Vec<Alert> {
    vec![
        get_consensus_block_number_progress_is_slow(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Sos,
        ),
        get_consensus_block_number_progress_is_slow(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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
            comparison_value: 3.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        AlertEnvFiltering::All,
    )
}

fn get_cende_write_blob_failure_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_cende_write_blob_failure_alert_vec() -> Vec<Alert> {
    vec![
        get_cende_write_blob_failure_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_cende_write_blob_failure_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

/// There was a round larger than zero in the last hour.
fn get_consensus_round_above_zero(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "consensus_round_above_zero",
        "Consensus round above zero",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_round_above_zero_vec() -> Vec<Alert> {
    vec![
        get_consensus_round_above_zero(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_consensus_round_above_zero(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

/// There were 5 times in the last 30 minutes that the round was larger than zero.
// TODO(guy.f): Create a new histogram type metric for measuring how many times we reached each
// round and use it here.
fn get_consensus_round_above_zero_multiple_times(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "consensus_round_above_zero_multiple_times",
        "Consensus round above zero multiple times",
        AlertGroup::Consensus,
        format!("increase({}[30m])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_round_above_zero_multiple_times_vec() -> Vec<Alert> {
    vec![
        get_consensus_round_above_zero_multiple_times(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Sos,
        ),
        get_consensus_round_above_zero_multiple_times(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn build_idle_alert(
    alert_name: &str,
    alert_title: &str,
    alert_group: AlertGroup,
    metric_name_with_filter: &str,
) -> Alert {
    Alert::new(
        alert_name,
        alert_title,
        alert_group,
        format!("sum(increase({}[2m])) or vector(0)", metric_name_with_filter),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Sos,
        AlertEnvFiltering::All,
    )
}

fn get_http_server_no_successful_transactions() -> Alert {
    build_idle_alert(
        "http_server_no_successful_transactions",
        "http server no successful transactions",
        AlertGroup::HttpServer,
        ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter(),
    )
}

fn get_gateway_add_tx_idle() -> Alert {
    build_idle_alert(
        "gateway_add_tx_idle_all_sources",
        "Gateway add_tx idle (all sources)",
        AlertGroup::Gateway,
        GATEWAY_TRANSACTIONS_RECEIVED.get_name_with_filter(),
    )
}

// TODO(shahak): add gateway latency alert

fn get_mempool_add_tx_idle() -> Alert {
    build_idle_alert(
        "mempool_add_tx_idle_all_sources",
        "Mempool add_tx idle (all sources)",
        AlertGroup::Mempool,
        MEMPOOL_TRANSACTIONS_RECEIVED.get_name_with_filter(),
    )
}

fn get_http_server_internal_error_ratio(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
            comparison_value: 0.01,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_http_server_internal_error_ratio_vec() -> Vec<Alert> {
    vec![
        get_http_server_internal_error_ratio(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_internal_error_ratio(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

/// Alert if we have no successful eth to strk rates data from the last hour.
fn get_eth_to_strk_success_count_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_eth_to_strk_success_count_alert_vec() -> Vec<Alert> {
    vec![
        get_eth_to_strk_success_count_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_eth_to_strk_success_count_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

/// Alert if had no successful l1 gas price scrape in the last hour.
fn get_l1_gas_price_scraper_success_count_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_l1_gas_price_scraper_success_count_alert_vec() -> Vec<Alert> {
    vec![
        get_l1_gas_price_scraper_success_count_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_l1_gas_price_scraper_success_count_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn get_l1_gas_price_provider_insufficient_history_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_l1_gas_price_provider_insufficient_history_alert_vec() -> Vec<Alert> {
    vec![
        get_l1_gas_price_provider_insufficient_history_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_l1_gas_price_provider_insufficient_history_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn get_l1_message_scraper_no_successes_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "l1_message_no_successes",
        "L1 message no successes",
        AlertGroup::L1GasPrice,
        format!("increase({}[5m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_l1_message_scraper_no_successes_alert_vec() -> Vec<Alert> {
    vec![
        get_l1_message_scraper_no_successes_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Sos,
        ),
        get_l1_message_scraper_no_successes_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
    ]
}

fn get_http_server_low_successful_transaction_rate(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "http_server_low_successful_transaction_rate",
        "http server low successful transaction rate",
        AlertGroup::HttpServer,
        format!(
            "increase({}[10m]) or vector(0)",
            ADDED_TRANSACTIONS_SUCCESS.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 5.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_http_server_low_successful_transaction_rate_vec() -> Vec<Alert> {
    vec![
        get_http_server_low_successful_transaction_rate(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_http_server_low_successful_transaction_rate(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_http_server_high_transaction_failure_ratio(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
            comparison_value: 0.2,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_http_server_high_transaction_failure_ratio_vec() -> Vec<Alert> {
    vec![
        get_http_server_high_transaction_failure_ratio(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_high_transaction_failure_ratio(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

// TODO(guy.f): consider uniting with regular tx failure rate.
// TODO(guyf.f): Change threshold to 0.05 after mainnet launch.
fn get_http_server_high_deprecated_transaction_failure_ratio() -> Alert {
    Alert::new(
        "http_server_high_deprecated_transaction_failure_ratio",
        "http server high deprecated transaction failure ratio",
        AlertGroup::HttpServer,
        format!(
            "increase({}[1h]) / clamp_min(increase({}[1h]), 1)",
            ADDED_TRANSACTIONS_DEPRECATED_ERROR.get_name_with_filter(),
            ADDED_TRANSACTIONS_TOTAL.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.1,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        AlertEnvFiltering::All,
    )
}

/// Triggers if the average latency of `add_tx` calls, across all HTTP servers, exceeds 2 seconds
/// over a 2-minute window.
fn get_http_server_avg_add_tx_latency_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    let sum_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_sum_with_filter();
    let count_metric = HTTP_SERVER_ADD_TX_LATENCY.get_name_count_with_filter();

    Alert::new(
        "http_server_avg_add_tx_latency",
        "High HTTP server average add_tx latency",
        AlertGroup::HttpServer,
        format!("rate({sum_metric}[2m]) / rate({count_metric}[2m])"),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_http_server_avg_add_tx_latency_alert_vec() -> Vec<Alert> {
    vec![
        get_http_server_avg_add_tx_latency_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_avg_add_tx_latency_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

/// Triggers when the slowest 5% of transactions for a specific HTTP server are taking longer than 2
/// seconds over a 5-minute window.
fn get_http_server_p95_add_tx_latency_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_http_server_p95_add_tx_latency_alert_vec() -> Vec<Alert> {
    vec![
        get_http_server_p95_add_tx_latency_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_http_server_p95_add_tx_latency_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn get_mempool_pool_size_increase(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_mempool_pool_size_increase_vec() -> Vec<Alert> {
    vec![
        get_mempool_pool_size_increase(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_mempool_pool_size_increase(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_mempool_transaction_drop_ratio(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
            // TODO(leo): Decide on the final ratio and who should be alerted.
            comparison_value: 0.2,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_mempool_transaction_drop_ratio_vec() -> Vec<Alert> {
    vec![
        get_mempool_transaction_drop_ratio(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_mempool_transaction_drop_ratio(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_consensus_round_high(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "consensus_round_high",
        "Consensus round high",
        AlertGroup::Consensus,
        format!("max_over_time({}[2m])", CONSENSUS_ROUND.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 20.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_round_high_vec() -> Vec<Alert> {
    vec![
        get_consensus_round_high(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Sos),
        get_consensus_round_high(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn get_state_sync_lag(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_state_sync_lag_vec() -> Vec<Alert> {
    vec![
        get_state_sync_lag(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Regular),
        get_state_sync_lag(AlertEnvFiltering::TestnetStyleAlerts, AlertSeverity::DayOnly),
    ]
}

fn get_state_sync_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name,
        "State sync stuck",
        AlertGroup::StateSync,
        format!(
            "increase({}[{}s])",
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter(),
            duration.as_secs()
        ), // Alert is triggered when the class manager marker is not updated for {duration}s
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_state_sync_stuck_vec() -> Vec<Alert> {
    vec![
        get_state_sync_stuck(
            "state_sync_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
        get_state_sync_stuck(
            "state_sync_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::DayOnly,
        ),
        get_state_sync_stuck(
            "state_sync_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}

fn get_batched_transactions_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name,
        "Batched transactions stuck",
        AlertGroup::Batcher,
        format!(
            "changes({}[{}s])",
            BATCHED_TRANSACTIONS.get_name_with_filter(),
            duration.as_secs()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_batched_transactions_stuck_vec() -> Vec<Alert> {
    vec![
        get_batched_transactions_stuck(
            "batched_transactions_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Sos,
        ),
        get_batched_transactions_stuck(
            "batched_transactions_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::DayOnly,
        ),
        get_batched_transactions_stuck(
            "batched_transactions_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}

/// No preconfirmed block was written in the last 10 minutes.
fn get_preconfirmed_block_not_written(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "preconfirmed_block_not_written",
        "Preconfirmed block not written",
        AlertGroup::Batcher,
        format!("increase({}[2m])", PRECONFIRMED_BLOCK_WRITTEN.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_preconfirmed_block_not_written_vec() -> Vec<Alert> {
    vec![
        get_preconfirmed_block_not_written(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_preconfirmed_block_not_written(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_consensus_p2p_peer_down(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "consensus_p2p_peer_down",
        "Consensus p2p peer down",
        AlertGroup::Consensus,
        format!("max_over_time({}[2m])", CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_p2p_peer_down_vec() -> Vec<Alert> {
    vec![
        get_consensus_p2p_peer_down(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Sos),
        get_consensus_p2p_peer_down(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

fn get_consensus_p2p_not_enough_peers_for_quorum(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        alert_name,
        "Consensus p2p not enough peers for quorum",
        AlertGroup::Consensus,
        format!(
            "max_over_time({}[{}s])",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter(),
            duration.as_secs()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators and
            // assume_no_malicious_validators
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_consensus_p2p_not_enough_peers_for_quorum_vec() -> Vec<Alert> {
    vec![
        get_consensus_p2p_not_enough_peers_for_quorum(
            "consensus_p2p_not_enough_peers_for_quorum",
            AlertEnvFiltering::MainnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Sos,
        ),
        get_consensus_p2p_not_enough_peers_for_quorum(
            "consensus_p2p_not_enough_peers_for_quorum",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::WorkingHours,
        ),
        get_consensus_p2p_not_enough_peers_for_quorum(
            "consensus_p2p_not_enough_peers_for_quorum_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
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

fn get_mempool_p2p_peer_down(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        "mempool_p2p_peer_down",
        "Mempool p2p peer down",
        AlertGroup::Mempool,
        format!("max_over_time({}[2m])", MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            comparison_value: 2.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        alert_env_filtering,
    )
}

fn get_mempool_p2p_peer_down_vec() -> Vec<Alert> {
    vec![
        get_mempool_p2p_peer_down(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Regular),
        get_mempool_p2p_peer_down(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
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

fn get_mempool_evictions_count_alert(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
        alert_severity,
        alert_env_filtering,
    )
}

fn get_mempool_evictions_count_alert_vec() -> Vec<Alert> {
    vec![
        get_mempool_evictions_count_alert(
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Regular,
        ),
        get_mempool_evictions_count_alert(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
    ]
}

fn get_general_pod_state_not_ready() -> Alert {
    Alert::new(
        "pod_state_not_ready",
        "Pod State Not Ready",
        AlertGroup::General,
        // Checks if a container in a pod is not ready (status_ready < 1).
        // Triggers when at least one container is unhealthy or not passing readiness probes.
        format!("kube_pod_container_status_ready{}", metric_label_filter!()),
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

fn get_general_pod_state_crashloopbackoff() -> Alert {
    // Adding a 'reason' label to the metric label filter for 'CrashLoopBackOf' failures.
    // This is done by replacing the trailing '}' with ', reason="CrashLoopBackOff"}'.
    let metric_label_filter_with_reason = format!(
        "{}, reason=\"CrashLoopBackOff\"}}",
        metric_label_filter!().strip_suffix("}").expect("Metric label filter should end with a }")
    );
    Alert::new(
        "pod_state_crashloopbackoff",
        "Pod State CrashLoopBackOff",
        AlertGroup::General,
        format!(
            // Convert "NoData" to 0 using `absent`.
            "sum by(container, pod, namespace) (kube_pod_container_status_waiting_reason{}) or \
             absent(kube_pod_container_status_waiting_reason{}) * 0",
            metric_label_filter_with_reason, metric_label_filter_with_reason,
        ),
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

fn get_general_pod_memory_utilization(
    name: &str,
    title: &str,
    comparison_value: f64,
    severity: AlertSeverity,
) -> Alert {
    Alert::new(
        name,
        title,
        AlertGroup::General,
        format!(
            // Calculates the memory usage percentage of each container in a pod, relative to its
            // memory limit. This expression compares the actual memory usage
            // (working_set_bytes) of containers against their defined memory limits
            // (spec_memory_limit_bytes), and returns the result as a percentage.
            "max(container_memory_working_set_bytes{0}) by (container, pod, namespace) / \
             max(container_spec_memory_limit_bytes{0}) by (container, pod, namespace) * 100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        severity,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_memory_utilization_vec() -> Vec<Alert> {
    vec![
        get_general_pod_memory_utilization(
            "pod_state_high_memory_utilization",
            "Pod High Memory Utilization ( >70% )",
            70.0,
            AlertSeverity::DayOnly,
        ),
        get_general_pod_memory_utilization(
            "pod_state_critical_memory_utilization",
            "Pod Critical Memory Utilization ( >85% )",
            85.0,
            AlertSeverity::Regular,
        ),
    ]
}

fn get_general_pod_high_cpu_utilization() -> Alert {
    Alert::new(
        "pod_high_cpu_utilization",
        "Pod High CPU Utilization ( >90% )",
        AlertGroup::General,
        format!(
            // Calculates CPU usage rate over 2 minutes per container, compared to its defined CPU
            // quota. Showing CPU pressure.
            "max(irate(container_cpu_usage_seconds_total{0}[2m])) by (container, pod, namespace) \
             / (max(container_spec_cpu_quota{0}/100000) by (container, pod, namespace)) * 100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 90.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_disk_utilization(
    name: &str,
    title: &str,
    comparison_value: f64,
    severity: AlertSeverity,
) -> Alert {
    Alert::new(
        name,
        title,
        AlertGroup::General,
        format!(
            "max by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}) / (min \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_available_bytes{0}) + max \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}))*100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        severity,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_disk_utilization_vec() -> Vec<Alert> {
    vec![
        get_general_pod_disk_utilization(
            "pod_state_high_disk_utilization",
            "Pod High Disk Utilization ( >70% )",
            70.0,
            AlertSeverity::DayOnly,
        ),
        get_general_pod_disk_utilization(
            "pod_state_critical_disk_utilization",
            "Pod Critical Disk Utilization ( >90% )",
            90.0,
            AlertSeverity::Regular,
        ),
    ]
}

pub fn get_apollo_alerts(alert_env_filtering: AlertEnvFiltering) -> Alerts {
    // TODO(guy.f): Split the alerts into separate sub-modules.
    let mut alerts = vec![
        get_cende_write_blob_failure_once_alert(),
        get_cende_write_prev_height_blob_latency_too_high(),
        get_consensus_conflicting_votes(),
        get_consensus_decisions_reached_by_consensus_ratio(),
        get_consensus_inbound_stream_evicted_alert(),
        get_consensus_l1_gas_price_provider_failure(),
        get_consensus_l1_gas_price_provider_failure_once(),
        get_consensus_p2p_disconnections(),
        get_consensus_votes_num_sent_messages_alert(),
        get_eth_to_strk_error_count_alert(),
        get_gateway_add_tx_idle(),
        get_general_pod_state_not_ready(),
        get_general_pod_state_crashloopbackoff(),
        get_general_pod_high_cpu_utilization(),
        get_http_server_high_deprecated_transaction_failure_ratio(),
        get_http_server_internal_error_once(),
        get_http_server_no_successful_transactions(),
        get_l1_gas_price_reorg_detected_alert(),
        get_l1_gas_price_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_reorg_detected_alert(),
        get_mempool_add_tx_idle(),
        get_mempool_p2p_disconnections(),
        get_native_compilation_error_increase(),
    ];

    alerts.append(&mut get_batched_transactions_stuck_vec());
    alerts.append(&mut get_consensus_block_number_progress_is_slow_vec());
    alerts.append(&mut get_cende_write_blob_failure_alert_vec());
    alerts.append(&mut get_consensus_block_number_stuck_vec());
    alerts.append(&mut get_consensus_p2p_not_enough_peers_for_quorum_vec());
    alerts.append(&mut get_consensus_p2p_peer_down_vec());
    alerts.append(&mut get_consensus_round_above_zero_vec());
    alerts.append(&mut get_consensus_round_above_zero_multiple_times_vec());
    alerts.append(&mut get_consensus_round_high_vec());
    alerts.append(&mut get_eth_to_strk_success_count_alert_vec());
    alerts.append(&mut get_general_pod_memory_utilization_vec());
    alerts.append(&mut get_general_pod_disk_utilization_vec());
    alerts.append(&mut get_http_server_avg_add_tx_latency_alert_vec());
    alerts.append(&mut get_http_server_high_transaction_failure_ratio_vec());
    alerts.append(&mut get_http_server_internal_error_ratio_vec());
    alerts.append(&mut get_http_server_low_successful_transaction_rate_vec());
    alerts.append(&mut get_http_server_p95_add_tx_latency_alert_vec());
    alerts.append(&mut get_l1_gas_price_provider_insufficient_history_alert_vec());
    alerts.append(&mut get_l1_gas_price_scraper_success_count_alert_vec());
    alerts.append(&mut get_l1_message_scraper_no_successes_alert_vec());
    alerts.append(&mut get_mempool_evictions_count_alert_vec());
    alerts.append(&mut get_mempool_p2p_peer_down_vec());
    alerts.append(&mut get_mempool_pool_size_increase_vec());
    alerts.append(&mut get_mempool_transaction_drop_ratio_vec());
    alerts.append(&mut get_preconfirmed_block_not_written_vec());
    alerts.append(&mut get_state_sync_lag_vec());
    alerts.append(&mut get_state_sync_stuck_vec());

    Alerts::new(alerts, alert_env_filtering)
}
