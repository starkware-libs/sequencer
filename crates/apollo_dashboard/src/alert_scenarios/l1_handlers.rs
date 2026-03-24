use apollo_l1_events::metrics::L1_MESSAGE_SCRAPER_SUCCESS_COUNT;
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    EvaluationRate,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

pub(crate) fn get_l1_message_scraper_no_successes_alert() -> Alert {
    const ALERT_NAME: &str = "l1_message_no_successes";
    Alert::new(
        ALERT_NAME,
        "L1 message no successes",
        EvaluationRate::Default,
        format!("increase({}[5m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
