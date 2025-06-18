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
use apollo_consensus_manager::metrics::CONSENSUS_VOTES_NUM_SENT_MESSAGES;
use apollo_consensus_orchestrator::metrics::{
    CENDE_WRITE_BLOB_FAILURE,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR,
};
use apollo_gateway::metrics::{GATEWAY_ADD_TX_LATENCY, GATEWAY_TRANSACTIONS_RECEIVED};
use apollo_http_server::metrics::ADDED_TRANSACTIONS_TOTAL;
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
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

fn get_gateway_add_tx_latency_increase() -> Alert {
    Alert {
        name: "gateway_add_tx_latency_increase",
        title: "Gateway avg add_tx latency increase",
        alert_group: AlertGroup::Gateway,
        expr: format!(
            "sum(rate({}[1m]))/sum(rate({}[1m]))",
            GATEWAY_ADD_TX_LATENCY.get_name_sum_with_filter(),
            GATEWAY_ADD_TX_LATENCY.get_name_count_with_filter(),
        ),
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
            comparison_value: 0.000001,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
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

fn get_l1_gas_price_scraper_baselayer_error_count_alert() -> Alert {
    Alert {
        name: "l1_message_scraper_baselayer_error_count",
        title: "L1 message scraper baselayer error count",
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

// The rate of add_txs is lower than the rate of transactions inserted into a block since this node
// is not always the proposer.
fn get_mempool_get_txs_size_drop() -> Alert {
    Alert {
        name: "mempool_get_txs_size_drop",
        title: "Mempool get_txs size drop",
        alert_group: AlertGroup::Mempool,
        expr: format!("avg_over_time({}[20m])", MEMPOOL_GET_TXS_SIZE.get_name_with_filter()),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 0.01,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: PENDING_DURATION_DEFAULT,
        evaluation_interval_sec: EVALUATION_INTERVAL_SEC_DEFAULT,
        severity: AlertSeverity::Regular,
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

fn get_consensus_round_high_avg() -> Alert {
    Alert {
        name: "consensus_round_high_avg",
        title: "Consensus round high average",
        alert_group: AlertGroup::Consensus,
        expr: format!("avg_over_time({}[10m])", CONSENSUS_ROUND.get_name_with_filter()),
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
            "min_over_time(({} - {})[5m])",
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

pub fn get_apollo_alerts() -> Alerts {
    Alerts::new(vec![
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
        get_consensus_round_above_zero(),
        get_consensus_round_high_avg(),
        get_consensus_validate_proposal_failed_alert(),
        get_consensus_votes_num_sent_messages_alert(),
        get_gateway_add_tx_latency_increase(),
        get_gateway_add_tx_idle(),
        get_http_server_idle(),
        get_l1_gas_price_provider_insufficient_history_alert(),
        get_l1_gas_price_reorg_detected_alert(),
        get_l1_gas_price_scraper_baselayer_error_count_alert(),
        get_eth_to_strk_error_count_alert(),
        get_l1_message_scraper_baselayer_error_count_alert(),
        get_l1_message_scraper_reorg_detected_alert(),
        get_mempool_add_tx_idle(),
        get_mempool_get_txs_size_drop(),
        get_mempool_pool_size_increase(),
        get_native_compilation_error_increase(),
        get_preconfirmed_block_not_written(),
        get_state_sync_lag(),
        get_state_sync_stuck(),
    ])
}
