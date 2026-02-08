use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND_ABOVE_ZERO};
use apollo_consensus_manager::metrics::CONSENSUS_NUM_CONNECTED_PEERS;
use apollo_consensus_orchestrator::metrics::CENDE_WRITE_BLOB_FAILURE;
use apollo_infra_utils::template::Template;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::{
    format_sampling_window,
    ComparisonValueOrPlaceholder,
    ExpressionOrExpressionWithPlaceholder,
};
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// There was a consensus round number higher than zero.
pub(crate) fn get_consensus_round_above_zero() -> Alert {
    Alert::new(
        "consensus_round_above_zero",
        "Consensus round above zero",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_round_above_zero_multiple_times(alert_severity: AlertSeverity) -> Alert {
    const ALERT_NAME: &str = "consensus_round_above_zero_multiple_times";
    let expr_template_string =
        format!("increase({}[{{}}s])", CONSENSUS_ROUND_ABOVE_ZERO.get_name_with_filter());
    Alert::new(
        ALERT_NAME,
        "Consensus round above zero multiple times",
        AlertGroup::Consensus,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(ALERT_NAME)],
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            ComparisonValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_consensus_round_above_zero_multiple_times_vec() -> Vec<Alert> {
    vec![get_consensus_round_above_zero_multiple_times(AlertSeverity::Sos)]
}

fn get_cende_write_blob_failure_alert(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "cende_write_blob_failure",
        "Cende write blob failure",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_cende_write_blob_failure_alert_vec() -> Vec<Alert> {
    vec![get_cende_write_blob_failure_alert(AlertSeverity::DayOnly)]
}

fn get_consensus_p2p_peer_down(alert_severity: AlertSeverity) -> Alert {
    Alert::new(
        "consensus_p2p_peer_down",
        "Consensus p2p peer down",
        AlertGroup::Consensus,
        format!("max_over_time({}[2m])", CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter()),
        vec![AlertCondition::new(
            AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators
            2.0,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::Applicable,
    )
}

pub(crate) fn get_consensus_p2p_peer_down_vec() -> Vec<Alert> {
    vec![get_consensus_p2p_peer_down(AlertSeverity::Regular)]
}

pub(crate) fn get_cende_write_blob_failure_once_alert() -> Alert {
    Alert::new(
        "cende_write_blob_failure_once",
        "Cende write blob failure once",
        AlertGroup::Consensus,
        format!("increase({}[1h])", CENDE_WRITE_BLOB_FAILURE.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Informational,
        ObserverApplicability::NotApplicable,
    )
}

fn get_consensus_block_number_progress_is_slow(alert_severity: AlertSeverity) -> Alert {
    const ALERT_NAME: &str = "get_consensus_block_number_progress_is_slow";
    Alert::new(
        ALERT_NAME,
        "Consensus block number progress is slow",
        AlertGroup::Consensus,
        format!(
            "sum(increase({}[2m])) or vector(0)",
            CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::LessThan,
            ComparisonValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::Applicable,
    )
}

pub(crate) fn get_consensus_block_number_progress_is_slow_vec() -> Vec<Alert> {
    vec![get_consensus_block_number_progress_is_slow(AlertSeverity::Regular)]
}
