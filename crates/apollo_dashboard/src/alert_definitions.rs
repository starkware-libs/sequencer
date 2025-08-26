use apollo_consensus::metrics::{
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_INBOUND_STREAM_EVICTED,
};
use apollo_consensus_manager::metrics::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use apollo_consensus_orchestrator::metrics::{
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR,
    CONSENSUS_PROPOSAL_FIN_MISMATCH,
};
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
};
use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
};
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_NUM_CONNECTED_PEERS;
use blockifier::metrics::NATIVE_COMPILATION_ERROR;

use crate::alert_scenarios::block_production_delay::{
    get_cende_write_blob_failure_alert_vec,
    get_cende_write_blob_failure_once_alert,
    get_consensus_block_number_progress_is_slow_vec,
    get_consensus_p2p_peer_down_vec,
    get_consensus_round_above_zero_multiple_times_vec,
    get_consensus_round_above_zero_vec,
};
use crate::alert_scenarios::block_production_halt::{
    get_batched_transactions_stuck_vec,
    get_consensus_block_number_stuck_vec,
    get_consensus_p2p_not_enough_peers_for_quorum_vec,
    get_consensus_round_high_vec,
};
use crate::alert_scenarios::infra_alerts::{
    get_general_pod_disk_utilization_vec,
    get_general_pod_high_cpu_utilization,
    get_general_pod_memory_utilization_vec,
    get_general_pod_state_crashloopbackoff,
    get_general_pod_state_not_ready,
};
use crate::alert_scenarios::l1_gas_prices::{
    get_eth_to_strk_success_count_alert_vec,
    get_l1_gas_price_provider_insufficient_history_alert_vec,
    get_l1_gas_price_scraper_success_count_alert_vec,
};
use crate::alert_scenarios::l1_handlers::get_l1_message_scraper_no_successes_alert_vec;
use crate::alert_scenarios::mempool_size::{
    get_mempool_evictions_count_alert_vec,
    get_mempool_pool_size_increase_vec,
};
use crate::alert_scenarios::preconfirmed::get_preconfirmed_block_not_written_vec;
use crate::alert_scenarios::sync_halt::{get_state_sync_lag_vec, get_state_sync_stuck_vec};
use crate::alert_scenarios::tps::{
    get_gateway_add_tx_idle,
    get_gateway_low_successful_transaction_rate_vec,
    get_http_server_no_successful_transactions,
    get_mempool_add_tx_idle,
};
use crate::alert_scenarios::transaction_delays::{
    get_http_server_avg_add_tx_latency_alert_vec,
    get_http_server_p95_add_tx_latency_alert_vec,
    get_mempool_p2p_peer_down_vec,
};
use crate::alert_scenarios::transaction_failures::{
    get_http_server_high_deprecated_transaction_failure_ratio,
    get_http_server_high_transaction_failure_ratio_vec,
    get_http_server_internal_error_once,
    get_http_server_internal_error_ratio_vec,
    get_mempool_transaction_drop_ratio_vec,
};
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    Alerts,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

pub fn get_dev_alerts_json_path(alert_env_filtering: AlertEnvFiltering) -> String {
    format!("crates/apollo_dashboard/resources/dev_grafana_alerts_{}.json", alert_env_filtering)
}

// TODO(guy.f): Can we have spaces in the alert names? If so, do we want to make the alert name and
// title the same?

// TODO(shahak): Move the remaining alerts here into modules.

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

fn get_consensus_proposal_fin_mismatch_once() -> Alert {
    Alert::new(
        "consensus_proposal_fin_mismatch_once",
        "Consensus proposal fin mismatch occurred",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_PROPOSAL_FIN_MISMATCH.get_name_with_filter()),
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
            "sum(increase({}[1m])) + vector(0)",
            L1_MESSAGE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Sos,
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

pub fn get_apollo_alerts(alert_env_filtering: AlertEnvFiltering) -> Alerts {
    let mut alerts = vec![
        get_consensus_proposal_fin_mismatch_once(),
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
    alerts.append(&mut get_gateway_low_successful_transaction_rate_vec());
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
