use apollo_l1_provider::metrics::L1_MESSAGE_SCRAPER_SUCCESS_COUNT;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert, AlertComparisonOp, AlertCondition, AlertEnvFiltering, AlertGroup, AlertLogicalOp,
    AlertSeverity, EVALUATION_INTERVAL_SEC_DEFAULT, ObserverApplicability,
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
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
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
