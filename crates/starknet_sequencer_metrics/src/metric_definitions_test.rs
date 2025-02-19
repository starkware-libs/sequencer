use std::collections::HashSet;

use crate::metric_definitions::{
    ALL_METRIC_COUNTERS,
    ALL_METRIC_GAUGES,
    ALL_METRIC_LABELED_COUNTERS,
};

// Tests that the metric names are unique.
#[test]
fn metric_names_no_duplications() {
    let all_metric_names: Vec<&'static str> = ALL_METRIC_COUNTERS
        .iter()
        .map(|metric| metric.get_name())
        .chain(ALL_METRIC_LABELED_COUNTERS.iter().map(|metric| metric.get_name()))
        .chain(ALL_METRIC_GAUGES.iter().map(|metric| metric.get_name()))
        .collect();

    let mut metric_names: HashSet<&'static str> = HashSet::new();
    for counter_name in all_metric_names {
        assert!(metric_names.insert(counter_name), "Duplicated metric name: {}", counter_name);
    }
}
