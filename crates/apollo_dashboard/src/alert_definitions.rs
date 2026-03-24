use apollo_batcher::metrics::{BATCHER_INFRA_METRICS, BATCHER_L1_EVENTS_PROVIDER_ERRORS};
use apollo_class_manager::metrics::CLASS_MANAGER_INFRA_METRICS;
use apollo_committer::metrics::COMMITTER_INFRA_METRICS;
use apollo_compile_to_casm::metrics::SIERRA_COMPILER_INFRA_METRICS;
use apollo_config_manager::metrics::CONFIG_MANAGER_INFRA_METRICS;
use apollo_consensus::metrics::{
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_INBOUND_PEER_EVICTED,
    CONSENSUS_INBOUND_STREAM_BUFFER_FULL,
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
    CONSENSUS_RETROSPECTIVE_BLOCK_HASH_MISMATCH,
};
use apollo_gateway::metrics::{GATEWAY_INFRA_METRICS, GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE};
use apollo_l1_events::metrics::{
    L1_EVENTS_INFRA_METRICS,
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
};
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    L1_GAS_PRICE_INFRA_METRICS,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
};
use apollo_mempool::metrics::MEMPOOL_INFRA_METRICS;
use apollo_mempool_p2p::metrics::{MEMPOOL_P2P_INFRA_METRICS, MEMPOOL_P2P_NUM_CONNECTED_PEERS};
use apollo_metrics::metrics::MetricQueryName;
use apollo_signature_manager::metrics::SIGNATURE_MANAGER_INFRA_METRICS;
use apollo_staking::metrics::STAKING_CURRENT_EPOCH_ID;
use apollo_state_sync_metrics::metrics::STATE_SYNC_INFRA_METRICS;
use apollo_storage::metrics::{
    BATCHER_STORAGE_OPEN_READ_TRANSACTIONS,
    CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS,
    SYNC_STORAGE_OPEN_READ_TRANSACTIONS,
};
use blockifier::metrics::NATIVE_COMPILATION_ERROR;

use crate::alert_scenarios::block_production_delay::{
    consensus_block_number_progress_is_slow,
    get_cende_write_blob_failure_alert,
    get_cende_write_blob_failure_once_alert,
    get_consensus_p2p_peer_down,
    get_consensus_round_above_zero,
    get_consensus_round_above_zero_multiple_times,
};
use crate::alert_scenarios::block_production_halt::{
    get_batched_transactions_stuck_vec,
    get_consensus_block_number_stuck_vec,
    get_consensus_p2p_not_enough_peers_for_quorum_vec,
    get_consensus_round_high,
};
use crate::alert_scenarios::config_manager::get_config_manager_update_error_increase;
use crate::alert_scenarios::infra_alerts::{
    get_general_pod_disk_utilization_vec,
    get_general_pod_high_cpu_utilization,
    get_general_pod_memory_utilization_vec,
    get_general_pod_state_crashloopbackoff,
    get_general_pod_state_not_ready,
    get_periodic_ping,
};
use crate::alert_scenarios::l1_gas_prices::{
    get_eth_to_strk_success_count_alert,
    get_l1_gas_price_provider_insufficient_history_alert,
    get_l1_gas_price_scraper_success_count_alert,
};
use crate::alert_scenarios::l1_handlers::get_l1_message_scraper_no_successes_alert;
use crate::alert_scenarios::mempool_size::{
    get_mempool_evictions_count_alert,
    get_mempool_pool_size_increase,
};
use crate::alert_scenarios::preconfirmed::get_preconfirmed_block_not_written;
use crate::alert_scenarios::remote_server_connections::get_remote_server_number_of_connections_alert;
use crate::alert_scenarios::sync_halt::{get_state_sync_lag, get_state_sync_stuck_vec};
use crate::alert_scenarios::tps::{
    get_gateway_add_tx_idle,
    get_gateway_low_successful_transaction_rate,
    get_http_server_no_successful_transactions,
    get_mempool_add_tx_idle,
};
use crate::alert_scenarios::transaction_delays::{
    get_high_empty_blocks_ratio_alert,
    get_http_server_avg_add_tx_latency_alert,
    get_http_server_min_add_tx_latency_alert,
    get_http_server_p95_add_tx_latency_alert,
    get_mempool_p2p_peer_down,
};
use crate::alert_scenarios::transaction_failures::{
    get_http_server_high_deprecated_transaction_failure_ratio,
    get_http_server_high_transaction_failure_ratio,
    get_http_server_internal_error_once,
    get_http_server_internal_error_ratio,
    get_mempool_transaction_drop_ratio,
};
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    Alerts,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

pub fn get_dev_alerts_json_path() -> String {
    "crates/apollo_dashboard/resources/dev_grafana_alerts.json".to_string()
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
            "(sum(increase({consensus}[10m])) or vector(0)) / \
             clamp_min((sum(increase({sync}[10m])) or vector(0)) + \
             (sum(increase({consensus}[10m])) or vector(0)), 1)",
            consensus = CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name_with_filter(),
            sync = CONSENSUS_DECISIONS_REACHED_BY_SYNC.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 0.5, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_inbound_stream_evicted_alert() -> Alert {
    Alert::new(
        "consensus_inbound_stream_evicted",
        "Consensus inbound stream evicted",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_INBOUND_STREAM_EVICTED.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_inbound_peer_evicted_alert() -> Alert {
    Alert::new(
        "consensus_inbound_peer_evicted",
        "Consensus inbound peer evicted",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_INBOUND_PEER_EVICTED.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_inbound_stream_buffer_full_alert() -> Alert {
    Alert::new(
        "consensus_inbound_stream_buffer_full",
        "Consensus inbound stream buffer full",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_INBOUND_STREAM_BUFFER_FULL.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_votes_num_sent_messages_alert() -> Alert {
    Alert::new(
        "consensus_votes_num_sent_messages",
        "Consensus votes num sent messages",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[20m])) or vector(0)",
            CONSENSUS_VOTES_NUM_SENT_MESSAGES.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 20.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_cende_write_prev_height_blob_latency_too_high() -> Alert {
    Alert::new(
        "cende_write_prev_height_blob_latency_too_high",
        "Cende write prev height blob latency too high",
        AlertGroup::Consensus,
        format!(
            "(sum(rate({}[20m])) or vector(0)) / clamp_min(sum(rate({}[20m])) or vector(0), \
             0.0000001)",
            CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_sum_with_filter(),
            CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY.get_name_count_with_filter(),
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 3.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_l1_gas_price_provider_failure() -> Alert {
    Alert::new(
        "consensus_l1_gas_price_provider_failure",
        "Consensus L1 gas price provider failure",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_l1_gas_price_provider_failure_once() -> Alert {
    Alert::new(
        "consensus_l1_gas_price_provider_failure_once",
        "Consensus L1 gas price provider failure once",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_proposal_fin_mismatch_once() -> Alert {
    Alert::new(
        "consensus_proposal_fin_mismatch_once",
        "Consensus proposal fin mismatch occurred",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            CONSENSUS_PROPOSAL_FIN_MISMATCH.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_conflicting_votes() -> Alert {
    Alert::new(
        "consensus_conflicting_votes",
        "Consensus conflicting votes",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[20m])) or vector(0)",
            CONSENSUS_CONFLICTING_VOTES.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        // TODO(matan): Increase severity once slashing is supported.
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_retrospective_block_hash_mismatch() -> Alert {
    Alert::new(
        "consensus_retrospective_block_hash_mismatch",
        "Mismatched retrospective block hashes between the state sync and the batcher",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[5m])) or vector(0)",
            CONSENSUS_RETROSPECTIVE_BLOCK_HASH_MISMATCH.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Sos,
        ObserverApplicability::Applicable,
    )
}

fn get_eth_to_strk_error_count_alert() -> Alert {
    Alert::new(
        "eth_to_strk_error_count",
        "Eth to Strk error count",
        AlertGroup::L1GasPrice,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            ETH_TO_STRK_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        "1m",
        20,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_l1_gas_price_scraper_baselayer_error_count_alert() -> Alert {
    Alert::new(
        "l1_gas_price_scraper_baselayer_error_count",
        "L1 gas price scraper baselayer error count",
        AlertGroup::L1GasPrice,
        format!(
            "sum(increase({}[5m])) or vector(0)",
            L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_l1_gas_price_reorg_detected_alert() -> Alert {
    Alert::new(
        "l1_gas_price_scraper_reorg_detected",
        "L1 gas price scraper reorg detected",
        AlertGroup::L1GasPrice,
        format!(
            "sum(increase({}[1m])) or vector(0)",
            L1_GAS_PRICE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_l1_message_scraper_baselayer_error_count_alert() -> Alert {
    Alert::new(
        "l1_message_scraper_baselayer_error_count",
        "L1 message scraper baselayer error count",
        AlertGroup::L1Messages,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 5.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_l1_message_scraper_reorg_detected_alert() -> Alert {
    Alert::new(
        "l1_message_scraper_reorg_detected",
        "L1 message scraper reorg detected",
        AlertGroup::L1Messages,
        format!(
            "sum(increase({}[1m])) or vector(0)",
            L1_MESSAGE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Sos,
        ObserverApplicability::NotApplicable,
    )
}

fn get_l1_events_provider_errors_alert() -> Alert {
    Alert::new(
        "batcher_l1_events_provider_errors",
        "Batcher L1 events provider errors",
        AlertGroup::Batcher,
        format!(
            "sum(increase({}[10m])) or vector(0)",
            BATCHER_L1_EVENTS_PROVIDER_ERRORS.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        // TODO(Arni): set a configurable severity, similar to `get_high_empty_blocks_ratio_alert`.
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_native_compilation_error_increase() -> Alert {
    Alert::new(
        "native_compilation_error",
        "Native compilation alert",
        AlertGroup::Batcher,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            NATIVE_COMPILATION_ERROR.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
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
            "(sum(changes({}[1h])) or vector(0)) / 2",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::Applicable,
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
            "(sum(changes({}[1h])) or vector(0)) / 2",
            MEMPOOL_P2P_NUM_CONNECTED_PEERS.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

const MAX_OPEN_READ_TRANSACTIONS: f64 = 7500.0;

fn create_storage_open_read_transactions_alert(storage_type: &str, metric_name: &str) -> Alert {
    Alert::new(
        format!("{storage_type}_storage_open_read_transactions"),
        format!("{storage_type} - High number of open read transactions"),
        AlertGroup::StateSync,
        format!("max_over_time({}[1m])", metric_name),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            MAX_OPEN_READ_TRANSACTIONS,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

fn get_sync_storage_open_read_transactions_alert() -> Alert {
    create_storage_open_read_transactions_alert(
        "sync",
        &SYNC_STORAGE_OPEN_READ_TRANSACTIONS.get_name_with_filter(),
    )
}

fn get_batcher_storage_open_read_transactions_alert() -> Alert {
    create_storage_open_read_transactions_alert(
        "batcher",
        &BATCHER_STORAGE_OPEN_READ_TRANSACTIONS.get_name_with_filter(),
    )
}

fn get_class_manager_storage_open_read_transactions_alert() -> Alert {
    create_storage_open_read_transactions_alert(
        "class_manager",
        &CLASS_MANAGER_STORAGE_OPEN_READ_TRANSACTIONS.get_name_with_filter(),
    )
}

fn get_gateway_proof_archive_write_failure() -> Alert {
    Alert::new(
        "gateway_proof_archive_write_failure",
        "Gateway proof archive (GCS) write failure",
        AlertGroup::Gateway,
        format!(
            "sum(increase({}[1h])) or vector(0)",
            GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if different nodes report different epoch IDs, indicating one or more nodes are out of
/// sync. A 5-minute pending duration accounts for Prometheus scrape latency and brief mismatches
/// during normal epoch transitions.
fn get_staking_epoch_id_mismatch_alert() -> Alert {
    Alert::new(
        "staking_epoch_id_mismatch",
        "Staking epoch ID mismatch between nodes",
        AlertGroup::Staking,
        format!(
            "max({epoch_id}) - min({epoch_id})",
            epoch_id = STAKING_CURRENT_EPOCH_ID.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        "5m",
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        ObserverApplicability::NotApplicable,
    )
}

fn get_all_remote_server_connection_alerts() -> Vec<Alert> {
    vec![
        get_remote_server_number_of_connections_alert(
            "batcher",
            AlertGroup::Batcher,
            BATCHER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "class_manager",
            AlertGroup::General,
            CLASS_MANAGER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "committer",
            AlertGroup::Consensus,
            COMMITTER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "config_manager",
            AlertGroup::General,
            CONFIG_MANAGER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "gateway",
            AlertGroup::Gateway,
            GATEWAY_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "l1_gas_price",
            AlertGroup::L1GasPrice,
            L1_GAS_PRICE_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "l1_events",
            AlertGroup::L1Messages,
            L1_EVENTS_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "mempool",
            AlertGroup::Mempool,
            MEMPOOL_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "mempool_p2p",
            AlertGroup::Mempool,
            MEMPOOL_P2P_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "sierra_compiler",
            AlertGroup::Batcher,
            SIERRA_COMPILER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "signature_manager",
            AlertGroup::Consensus,
            SIGNATURE_MANAGER_INFRA_METRICS.get_remote_server_metrics(),
        ),
        get_remote_server_number_of_connections_alert(
            "state_sync",
            AlertGroup::StateSync,
            STATE_SYNC_INFRA_METRICS.get_remote_server_metrics(),
        ),
    ]
}

pub fn get_apollo_alerts() -> Alerts {
    let mut alerts = vec![
        get_batcher_storage_open_read_transactions_alert(),
        get_class_manager_storage_open_read_transactions_alert(),
        get_config_manager_update_error_increase(),
        get_consensus_proposal_fin_mismatch_once(),
        get_cende_write_blob_failure_once_alert(),
        get_cende_write_prev_height_blob_latency_too_high(),
        get_consensus_conflicting_votes(),
        get_consensus_decisions_reached_by_consensus_ratio(),
        get_consensus_inbound_peer_evicted_alert(),
        get_consensus_inbound_stream_buffer_full_alert(),
        get_consensus_inbound_stream_evicted_alert(),
        get_consensus_l1_gas_price_provider_failure(),
        get_consensus_l1_gas_price_provider_failure_once(),
        get_consensus_p2p_disconnections(),
        get_consensus_retrospective_block_hash_mismatch(),
        get_consensus_round_above_zero(),
        get_consensus_votes_num_sent_messages_alert(),
        get_eth_to_strk_error_count_alert(),
        get_gateway_add_tx_idle(),
        get_gateway_proof_archive_write_failure(),
        get_general_pod_state_not_ready(),
        get_general_pod_state_crashloopbackoff(),
        get_general_pod_high_cpu_utilization(),
        get_http_server_high_deprecated_transaction_failure_ratio(),
        get_http_server_high_transaction_failure_ratio(),
        get_http_server_internal_error_once(),
        get_http_server_no_successful_transactions(),
        get_l1_events_provider_errors_alert(),
        get_l1_gas_price_reorg_detected_alert(),
        get_l1_gas_price_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_reorg_detected_alert(),
        get_mempool_add_tx_idle(),
        get_mempool_p2p_disconnections(),
        get_native_compilation_error_increase(),
        get_periodic_ping(),
        get_staking_epoch_id_mismatch_alert(),
        get_sync_storage_open_read_transactions_alert(),
    ];

    alerts.append(&mut get_batched_transactions_stuck_vec());
    alerts.push(consensus_block_number_progress_is_slow());
    alerts.push(get_cende_write_blob_failure_alert());
    alerts.append(&mut get_consensus_block_number_stuck_vec());
    alerts.append(&mut get_consensus_p2p_not_enough_peers_for_quorum_vec());
    alerts.push(get_consensus_p2p_peer_down());
    alerts.push(get_consensus_round_above_zero_multiple_times());
    alerts.push(get_consensus_round_high());
    alerts.push(get_eth_to_strk_success_count_alert());
    alerts.append(&mut get_general_pod_memory_utilization_vec());
    alerts.append(&mut get_general_pod_disk_utilization_vec());
    alerts.push(get_http_server_avg_add_tx_latency_alert());
    alerts.push(get_http_server_min_add_tx_latency_alert());
    alerts.push(get_http_server_internal_error_ratio());
    alerts.push(get_gateway_low_successful_transaction_rate());
    alerts.push(get_http_server_p95_add_tx_latency_alert());
    alerts.push(get_high_empty_blocks_ratio_alert());
    alerts.push(get_l1_gas_price_provider_insufficient_history_alert());
    alerts.push(get_l1_gas_price_scraper_success_count_alert());
    alerts.push(get_l1_message_scraper_no_successes_alert());
    alerts.push(get_mempool_evictions_count_alert());
    alerts.push(get_mempool_p2p_peer_down());
    alerts.push(get_mempool_pool_size_increase());
    alerts.push(get_mempool_transaction_drop_ratio());
    alerts.push(get_preconfirmed_block_not_written());
    alerts.append(&mut get_all_remote_server_connection_alerts());
    alerts.push(get_state_sync_lag());
    alerts.append(&mut get_state_sync_stuck_vec());

    Alerts::new(alerts, EVALUATION_INTERVAL_SEC_DEFAULT)
}
