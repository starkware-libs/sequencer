use std::collections::HashSet;

use starknet_sequencer_metrics::metric_definitions::{
    ALL_LABELED_METRIC_COUNTERS,
    ALL_METRIC_COUNTERS,
    ALL_METRIC_GAUGES,
};

// Tests that the metric names are unique.
#[test]
fn metric_names_no_duplications() {
    let all_metric_names = ALL_METRIC_COUNTERS
        .iter()
        .map(|metric| metric.get_name())
        .chain(ALL_LABELED_METRIC_COUNTERS.iter().map(|metric| metric.get_name()))
        .chain(ALL_METRIC_GAUGES.iter().map(|metric| metric.get_name()))
        .collect::<Vec<&'static str>>();
    let mut unique_metric_names: HashSet<&'static str> = HashSet::new();
    for metric_name in all_metric_names {
        assert!(unique_metric_names.insert(metric_name), "Duplicated metric name: {}", metric_name,);
    }
}
