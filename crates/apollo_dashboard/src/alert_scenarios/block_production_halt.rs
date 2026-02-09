use std::time::Duration;

use apollo_batcher::metrics::BATCHED_TRANSACTIONS;
use apollo_consensus::metrics::{CONSENSUS_BLOCK_NUMBER, CONSENSUS_ROUND};
use apollo_consensus_manager::metrics::CONSENSUS_NUM_CONNECTED_PEERS;
use apollo_infra_utils::template::Template;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::{
    ComparisonValueOrPlaceholder, ExpressionOrExpressionWithPlaceholder,
    SeverityValueOrPlaceholder, format_sampling_window,
};
use crate::alerts::{
    Alert, AlertComparisonOp, AlertCondition, AlertEnvFiltering, AlertGroup, AlertLogicalOp,
    AlertSeverity, EVALUATION_INTERVAL_SEC_DEFAULT, ObserverApplicability,
    PENDING_DURATION_DEFAULT, SECS_IN_MIN,
};

/// Block number is stuck for more than duration minutes.
fn get_consensus_block_number_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
    let expr_template_string = format!(
        "sum(increase({}[{{}}s])) or vector(0)",
        CONSENSUS_BLOCK_NUMBER.get_name_with_filter()
    );
    Alert::new(
        alert_name,
        "Consensus block number stuck",
        AlertGroup::Consensus,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(alert_name)],
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
        alert_env_filtering,
    )
}

pub(crate) fn get_consensus_block_number_stuck_vec() -> Vec<Alert> {
    vec![
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
            AlertSeverity::Sos,
        ),
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::DayOnly,
        ),
        get_consensus_block_number_stuck(
            "consensus_block_number_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
            AlertSeverity::Regular,
        ),
    ]
}

// TODO(Tsabary): settle all the required parameters that are different among envs using the
// placeholder mechanism.
// TODO(Tsabary): remove `AlertEnvFiltering` throughout and use the placeholder mechanism instead.

fn get_batched_transactions_stuck(
    alert_name: &'static str,
    alert_env_filtering: AlertEnvFiltering,
) -> Alert {
    let expr_template_string =
        format!("changes({}[{{}}s])", BATCHED_TRANSACTIONS.get_name_with_filter());
    Alert::new(
        alert_name,
        "Batched transactions stuck",
        AlertGroup::Batcher,
        ExpressionOrExpressionWithPlaceholder::Placeholder(
            Template::new(expr_template_string),
            vec![format_sampling_window(alert_name)],
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(alert_name.to_string()),
        ObserverApplicability::NotApplicable,
        alert_env_filtering,
    )
}

pub(crate) fn get_batched_transactions_stuck_vec() -> Vec<Alert> {
    vec![
        get_batched_transactions_stuck(
            "batched_transactions_stuck",
            AlertEnvFiltering::MainnetStyleAlerts,
        ),
        get_batched_transactions_stuck(
            "batched_transactions_stuck",
            AlertEnvFiltering::TestnetStyleAlerts,
        ),
        get_batched_transactions_stuck(
            "batched_transactions_stuck_long_time",
            AlertEnvFiltering::TestnetStyleAlerts,
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

fn get_consensus_round_high(
    alert_env_filtering: AlertEnvFiltering,
    alert_severity: AlertSeverity,
) -> Alert {
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
