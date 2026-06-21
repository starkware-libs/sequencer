use apollo_metrics::metrics::MetricQueryName;
use papyrus_base_layer::metrics::{
    ScraperLabel,
    L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS,
    LABEL_NAME_SCRAPER,
};
use strum::IntoEnumIterator;

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

// 10 minutes; change this constant and regenerate the dashboard to adjust the threshold.
const MAX_PRIMARY_DOWN_SECONDS: f64 = 600.0;

/// Returns one alert per scraper label (`l1_events`, `l1_gas_price`).
///
/// Each alert pins the `scraper` label to a single value and leaves the remaining per-node (per
/// pod) series intact — it deliberately does NOT aggregate across nodes. The query A is an instant
/// query, so it emits one sample per `(scraper, node)` series; Grafana's threshold expression then
/// evaluates each series independently and raises a separate alert instance for any node whose
/// primary endpoint has been down longer than the threshold. This gives per-scraper-per-node
/// alerting: one node's prolonged outage fires even while other nodes of the same scraper are
/// healthy (which a `max`/`avg` collapse across nodes would hide).
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
                alert_name.clone(),
                format!("Primary L1 endpoint down too long ({scraper_str})"),
                EvaluationRate::Default,
                format!("(time() - {metric}) and ({metric} > 0)"),
                vec![AlertCondition::new(
                    AlertComparisonOp::GreaterThan,
                    MAX_PRIMARY_DOWN_SECONDS,
                    AlertLogicalOp::And,
                )],
                PENDING_DURATION_DEFAULT,
                SeverityValueOrPlaceholder::Placeholder(alert_name),
                ObserverApplicability::NotApplicable,
            )
        })
        .collect()
}
