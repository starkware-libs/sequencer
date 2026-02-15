use std::time::Duration;

use apollo_batcher::metrics::BATCHED_TRANSACTIONS;
use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND};
use apollo_consensus_manager::metrics::CONSENSUS_NUM_CONNECTED_PEERS;
use apollo_infra_utils::template::Template;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::{
    format_sampling_window,
    ComparisonValueOrPlaceholder,
    ExpressionOrExpressionWithPlaceholder,
    SeverityValueOrPlaceholder,
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
    SECS_IN_MIN,
};

/// Block number is stuck for more than duration minutes.
fn get_consensus_block_number_stuck(title: &'static str, alert_severity: AlertSeverity) -> Alert {
    let name = title.to_lowercase().replace(' ', "_");
    let expr_template_string = format!(
        "sum(increase({}[{{}}s])) or vector(0)",
        CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
    );
    Alert::new(
        &name,
        title,
        AlertGroup::Consensus,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(&name)],
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_consensus_block_number_stuck_vec() -> Vec<Alert> {
    vec![
        get_consensus_block_number_stuck("Consensus Block Number Stuck", AlertSeverity::Sos),
        get_consensus_block_number_stuck(
            "Consensus Block Number Stuck Long Time",
            AlertSeverity::Regular,
        ),
    ]
}

fn get_batched_transactions_stuck(title: &'static str) -> Alert {
    let name = title.to_lowercase().replace(' ', "_");
    let expr_template_string =
        format!("changes({}[{{}}s])", BATCHED_TRANSACTIONS.get_name_with_filter());
    Alert::new(
        &name,
        title,
        AlertGroup::Batcher,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(&name)],
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(name.clone()),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_batched_transactions_stuck_vec() -> Vec<Alert> {
    vec![
        get_batched_transactions_stuck("Batched Transactions Stuck"),
        get_batched_transactions_stuck("Batched Transactions Stuck Long Time"),
    ]
}

fn get_consensus_p2p_not_enough_peers_for_quorum(
    title: &'static str,
    duration: Duration,
    alert_severity: AlertSeverity,
) -> Alert {
    Alert::new(
        title.to_lowercase().replace(' ', "_"),
        title,
        AlertGroup::Consensus,
        format!(
            "max_over_time({}[{}s])",
            CONSENSUS_NUM_CONNECTED_PEERS.get_name_with_filter(),
            duration.as_secs()
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::LessThan,
            // TODO(shahak): find a way to make this depend on num_validators and
            // assume_no_malicious_validators
            1.0,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::Applicable,
    )
}

pub(crate) fn get_consensus_p2p_not_enough_peers_for_quorum_vec() -> Vec<Alert> {
    vec![
        get_consensus_p2p_not_enough_peers_for_quorum(
            "Consensus P2P Not Enough Peers For Quorum",
            Duration::from_secs(2 * SECS_IN_MIN),
            AlertSeverity::Sos,
        ),
        get_consensus_p2p_not_enough_peers_for_quorum(
            "Consensus P2P Not Enough Peers For Quorum Long Time",
            Duration::from_secs(30 * SECS_IN_MIN),
            AlertSeverity::Regular,
        ),
    ]
}

fn get_consensus_round_high(alert_severity: AlertSeverity) -> Alert {
    const ALERT_NAME: &str = "consensus_round_high";
    Alert::new(
        ALERT_NAME,
        "Consensus round high",
        AlertGroup::Consensus,
        format!("max_over_time({}[2m])", CONSENSUS_ROUND.get_name_with_filter()),
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

pub(crate) fn get_consensus_round_high_vec() -> Vec<Alert> {
    vec![get_consensus_round_high(AlertSeverity::Sos)]
}
