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

/// Returns a query that calculates the number of seconds since the last event timestamp recorded
/// in a gauge (unix timestamp seconds).
///
/// Example output:
/// `time() - max(last_over_time(my_last_success_timestamp_seconds{...}[12h]))`
pub(crate) fn seconds_since_last_timestamp(metric: &dyn MetricQueryName) -> String {
    format!("time() - max(last_over_time({}[12h]))", metric.get_name_with_filter())
}

/// Narrows a labeled metric to one label value.
///
/// Example: `with_label(&m, "currency_pair", "eth_strk")` → `m{..., currency_pair="eth_strk"}`
pub(crate) fn with_label(metric: &dyn MetricQueryName, label: &str, value: &str) -> String {
    with_labels(metric, &[(label, value)])
}

/// Narrows a multi-dimensional labeled metric to one value per label.
pub(crate) fn with_labels(metric: &dyn MetricQueryName, labels: &[(&str, &str)]) -> String {
    let filters = labels
        .iter()
        .map(|(label, value)| format!("{label}=\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ");
    metric.get_name_with_filer_and_additional_fields(&filters)
}

/// `sum(increase())` of one series of a multi-dimensional labeled metric.
pub(crate) fn sum_increase_with_labels(
    metric: &dyn MetricQueryName,
    labels: &[(&str, &str)],
    duration: &str,
) -> String {
    format!("sum(increase({}[{}]))", with_labels(metric, labels), duration)
}

/// `increase()` of one label value of a labeled metric.
pub(crate) fn increase_with_label(
    metric: &dyn MetricQueryName,
    label: &str,
    value: &str,
    duration: &str,
) -> String {
    format!("increase({}[{}])", with_label(metric, label, value), duration)
}

/// `sum(increase())` of one label value of a labeled metric, aggregating across instances.
pub(crate) fn sum_increase_with_label(
    metric: &dyn MetricQueryName,
    label: &str,
    value: &str,
    duration: &str,
) -> String {
    format!("sum({})", increase_with_label(metric, label, value, duration))
}

/// Seconds since the last event timestamp recorded for one label value of a labeled gauge.
pub(crate) fn seconds_since_last_timestamp_with_label(
    metric: &dyn MetricQueryName,
    label: &str,
    value: &str,
) -> String {
    format!("time() - max(last_over_time({}[12h]))", with_label(metric, label, value))
}

/// Builds `sum(increase(<metric>[<duration>]))` for aggregating a counter across all instances.
///
/// Example: `sum_increase(&m, "1h")` → `sum(increase(my_counter{...}[1h]))`
pub(crate) fn sum_increase(metric: &dyn MetricQueryName, duration: &str) -> String {
    format!("sum({})", increase(metric, duration))
}

/// Returns `sum by (namespace, pod) (<inner>)` where `<inner>` is either
/// the raw metric query or `increase(<metric>[<duration>])`.
///
/// - `metric`: source metric (with any label filters).
/// - `display`: `DisplayMethod::Raw` or `Increase("<duration>")`.
///
/// NOTE: this sums over all the containers in the pod, which is usually just one. A different query
/// is required for a finer-grained resolution,
pub(crate) fn sum_by_pod(metric: &dyn MetricQueryName, display: DisplayMethod<'_>) -> String {
    sum_by_label(metric, "namespace, pod", display, false)
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
