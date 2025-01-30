use std::collections::HashSet;

use crate::metric_definitions::{ALL_METRIC_COUNTERS, ALL_METRIC_GAUGES};

// Tests that the metric names are unique.
#[test]
fn metric_names_no_duplications() {
    let mut metric_names: HashSet<&'static str> = HashSet::new();
    for counter_metric in ALL_METRIC_COUNTERS {
        assert!(
            metric_names.insert(counter_metric.get_name()),
            "Duplicated metric name: {}",
            counter_metric.get_name()
        );
    }
    for gauge_metric in ALL_METRIC_GAUGES {
        assert!(
            metric_names.insert(gauge_metric.get_name()),
            "Duplicated metric name: {}",
            gauge_metric.get_name()
        );
    }
}
