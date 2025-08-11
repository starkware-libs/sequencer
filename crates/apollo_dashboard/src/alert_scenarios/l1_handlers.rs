use apollo_l1_provider::metrics::L1_MESSAGE_SCRAPER_SUCCESS_COUNT;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

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

pub(crate) fn get_l1_message_scraper_no_successes_alert_vec() -> Vec<Alert> {
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
