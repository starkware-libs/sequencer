pub(crate) mod queries {
    use apollo_metrics::metrics::MetricCommon;

    pub(crate) fn increase(metric: &dyn MetricCommon, duration: &str) -> String {
        format!("increase({}[{}])", metric.get_name_with_filter(), duration)
    }
}

#[cfg(test)]
mod tests {
    use apollo_metrics::metrics::{MetricGauge, MetricScope};

    use super::queries;

    #[test]
    fn increase_formats_correctly() {
        let m = MetricGauge::new(MetricScope::Batcher, "testing", "Fake description");
        let q = queries::increase(&m, "5m");
        assert_eq!(q, r#"increase(testing{cluster=~"$cluster", namespace=~"$namespace"}[5m])"#);
    }
}
