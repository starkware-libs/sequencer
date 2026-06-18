use apollo_l1_events::metrics::{
    L1_MESSAGE_PROVIDER_OLDEST_PENDING_TX_L1_TIMESTAMP_SECONDS,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};

pub(crate) fn get_l1_message_scraper_no_successes_alert() -> Alert {
    const ALERT_NAME: &str = "l1_message_no_successes";
    Alert::new(
        ALERT_NAME,
        "L1 message no successes",
        EvaluationRate::Default,
        format!("increase({}[15m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// Fires when a single L1 handler transaction has been waiting in L1 (uncommitted to L2) for too
/// long. The provider exports the L1 block timestamp of the oldest pending tx, so the current age
/// is `time() - <timestamp>`. The `and (... > 0)` guard suppresses the alert when nothing is
/// pending (the metric reports 0 in that case, which would otherwise look infinitely old).
pub(crate) fn get_l1_handler_transaction_waiting_in_l1_alert() -> Alert {
    const ALERT_NAME: &str = "l1_handler_transaction_waiting_in_L1";
    // 10 minutes; well above the proposal cooldown, so normal cooldown waits don't trip it.
    const MAX_PENDING_SECONDS: f64 = 600.0;
    let oldest_pending_timestamp =
        L1_MESSAGE_PROVIDER_OLDEST_PENDING_TX_L1_TIMESTAMP_SECONDS.get_name_with_filter();
    Alert::new(
        ALERT_NAME,
        "L1 handler transaction waiting in L1 too long",
        EvaluationRate::Default,
        format!("(time() - {oldest_pending_timestamp}) and ({oldest_pending_timestamp} > 0)"),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            MAX_PENDING_SECONDS,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
