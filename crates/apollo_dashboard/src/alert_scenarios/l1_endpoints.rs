use apollo_metrics::metrics::MetricQueryName;
use papyrus_base_layer::metrics::L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS;

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

/// Fires when the primary L1 endpoint has been continuously down for too long. The metric reports
/// the Unix timestamp since which the primary endpoint has been non-functional for a given scraper;
/// 0 when healthy. The `and (... > 0)` guard suppresses the alert when the endpoint is healthy.
/// Because `get_name_with_filter()` does not pin the `scraper` label, Grafana evaluates this
/// expression per-series, so each scraper (`l1_events`, `l1_gas_price`) fires independently.
pub(crate) fn get_primary_l1_endpoint_down_too_long_alert() -> Alert {
    const ALERT_NAME: &str = "primary_l1_endpoint_down_too_long";
    // 10 minutes; change this constant and regenerate the dashboard to adjust the threshold.
    const MAX_PRIMARY_DOWN_SECONDS: f64 = 600.0;
    let down_since_timestamp =
        L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS.get_name_with_filter();
    Alert::new(
        ALERT_NAME,
        "Primary L1 endpoint down too long",
        EvaluationRate::Default,
        format!("(time() - {down_since_timestamp}) and ({down_since_timestamp} > 0)"),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            MAX_PRIMARY_DOWN_SECONDS,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}
