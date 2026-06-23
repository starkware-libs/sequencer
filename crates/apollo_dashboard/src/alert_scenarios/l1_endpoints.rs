use apollo_metrics::metrics::MetricQueryName;
use papyrus_base_layer::metrics::{
    ScraperLabel,
    L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS,
    LABEL_NAME_SCRAPER,
};
use strum::IntoEnumIterator;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};

// 10 minutes; change this constant and regenerate the dashboard to adjust the threshold.
const MAX_PRIMARY_DOWN_SECONDS: f64 = 600.0;

/// Returns one alert per scraper label (`l1_events`, `l1_gas_price`).
///
/// Alerting is already per apollo node: the monitoring build provisions each node's Grafana with
/// its own copy of the rule, scoped to that node's `namespace`/`cluster` (see
/// `inject_expr_placeholders` in the monitoring builder). So the rule only ever sees a single
/// node's series, and there is no cross-node aggregation to worry about.
///
/// Each alert pins the `scraper` label to a single value and wraps the remaining per-pod series in
/// `max(...)` to collapse the node's pods into one worst-case downtime value. This yields one
/// alert per `(scraper, node)`: a long outage on the node fires the alert, without emitting a
/// separate instance per pod.
pub(crate) fn get_primary_l1_endpoint_down_too_long_alerts() -> Vec<Alert> {
    ScraperLabel::iter()
        .map(|scraper| {
            let scraper_str: &'static str = scraper.into();
            let alert_name = format!("primary_l1_endpoint_down_too_long_{scraper_str}");
            let metric = L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS
                .get_name_with_filer_and_additional_fields(&format!(
                    "{LABEL_NAME_SCRAPER}=\"{scraper_str}\""
                ));
            Alert::new(
                alert_name,
                format!("Primary L1 endpoint down too long ({scraper_str})"),
                EvaluationRate::Default,
                format!("max((time() - {metric}) and ({metric} > 0))"),
                vec![AlertCondition::new(
                    AlertComparisonOp::GreaterThan,
                    MAX_PRIMARY_DOWN_SECONDS,
                    AlertLogicalOp::And,
                )],
                PENDING_DURATION_DEFAULT,
                // P4: fires only during official business hours (excludes nights/holidays).
                AlertSeverity::WorkingHours,
                ObserverApplicability::NotApplicable,
            )
        })
        .collect()
}
