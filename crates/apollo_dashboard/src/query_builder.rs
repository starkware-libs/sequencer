use apollo_metrics::metrics::MetricQueryName;
#[cfg(test)]
#[path = "query_builder_test.rs"]
pub mod query_builder_test;

pub(crate) const DEFAULT_DURATION: &str = "10m";
// Expands to the currently selected dashboard time range
pub(crate) const RANGE_DURATION: &str = "$__range";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DisplayMethod<'a> {
    Increase(&'a str), // duration
    Raw,
}

/// Builds `increase(<metric>[<duration>])` for counters.
///
/// - `metric`: source metric (with any label filters).
/// - `duration`: range window, e.g. `"5m"`, `"1h"`.
///
/// Example: `increase(m, "5m")` → `increase(http_requests_total{...}[5m])`
pub(crate) fn increase(metric: &dyn MetricQueryName, duration: &str) -> String {
    format!("increase({}[{}])", metric.get_name_with_filter(), duration)
}

/// Returns a query string that sums a metric **by a label**, optionally using
/// `increase()` and filtering zeros.
///
/// - `metric`: provides the metric.
/// - `label`: label key for `sum by (...)`.
/// - `display`: `DisplayMethod::Raw` or `Increase("5m")`.
/// - `filter_zeros`: if `true`, appends ` > 0`.
///
/// Example:
/// `sum_by_label(&m, "something", DisplayMethod::Increase("5m"), true)`
/// → `sum by (something) (increase(<metric>[5m])) > 0`
pub(crate) fn sum_by_label(
    metric: &dyn MetricQueryName,
    label: &str,
    display: DisplayMethod<'_>,
    filter_zeros: bool,
) -> String {
    let inner = match display {
        DisplayMethod::Increase(duration) => increase(metric, duration),
        DisplayMethod::Raw => metric.get_name_with_filter(),
    };
    let filter = if filter_zeros { " > 0" } else { "" };

    format!("sum by ({}) ({}){}", label, inner, filter)
}
