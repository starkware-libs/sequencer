use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND_ABOVE_ZERO};
use apollo_consensus_manager::metrics::CONSENSUS_NUM_CONNECTED_PEERS;
use apollo_consensus_orchestrator::metrics::CENDE_WRITE_BLOB_FAILURE;

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

/// The was a round larger than zero in the last hour.
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

pub(crate) fn get_consensus_round_above_zero_vec() -> Vec<Alert> {
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

pub(crate) fn get_consensus_round_above_zero_multiple_times_vec() -> Vec<Alert> {
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

pub(crate) fn get_cende_write_blob_failure_alert_vec() -> Vec<Alert> {
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

pub(crate) fn get_consensus_p2p_peer_down_vec() -> Vec<Alert> {
    vec![
        get_consensus_p2p_peer_down(AlertEnvFiltering::MainnetStyleAlerts, AlertSeverity::Sos),
        get_consensus_p2p_peer_down(
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::WorkingHours,
        ),
    ]
}

pub(crate) fn get_cende_write_blob_failure_once_alert() -> Alert {
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
            comparison_value: 10.0,
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
