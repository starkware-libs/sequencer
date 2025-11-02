use apollo_metrics::metrics::MetricCommon;

#[cfg(test)]
#[path = "query_builder_test.rs"]
pub mod query_builder_test;

pub(crate) const DEFAULT_DURATION: &str = "10m";

pub(crate) fn increase(metric: &dyn MetricCommon, duration: &str) -> String {
    format!("increase({}[{}])", metric.get_name_with_filter(), duration)
}
