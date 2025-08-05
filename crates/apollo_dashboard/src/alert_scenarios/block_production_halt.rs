use std::time::Duration;

use apollo_batcher::metrics::BATCHED_TRANSACTIONS;
use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND};
use apollo_consensus_manager::metrics::CONSENSUS_NUM_CONNECTED_PEERS;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
};

const PENDING_DURATION_DEFAULT: &str = "30s";
const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;
const SECS_IN_MIN: u64 = 60;

/// Block number is stuck for more than duration minutes.
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

pub(crate) fn get_consensus_block_number_stuck_vec() -> Vec<Alert> {
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

pub(crate) fn get_batched_transactions_stuck_vec() -> Vec<Alert> {
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

pub(crate) fn get_consensus_p2p_not_enough_peers_for_quorum_vec() -> Vec<Alert> {
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

pub(crate) fn get_consensus_block_number_progress_is_slow_vec() -> Vec<Alert> {
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

pub(crate) fn get_consensus_round_high_vec() -> Vec<Alert> {
    vec![
        get_consensus_round_high(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Sos),
        get_consensus_round_high(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}
