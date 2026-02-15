use apollo_l1_provider::metrics::L1_MESSAGE_SCRAPER_SUCCESS_COUNT;
use apollo_metrics::metrics::MetricQueryName;

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

fn get_l1_message_scraper_no_successes_alert(alert_severity: AlertSeverity) -> Alert {
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
    )
}

pub(crate) fn get_l1_message_scraper_no_successes_alert_vec() -> Vec<Alert> {
    vec![get_l1_message_scraper_no_successes_alert(AlertSeverity::Sos)]
}
