use std::collections::HashSet;

use starknet_sequencer_metrics::metric_definitions::ALL_METRICS;

#[test]
fn metric_names_no_duplications() {
    let mut unique_metric_names: HashSet<&'static str> = HashSet::new();
    for metric_name in ALL_METRICS {
        assert!(unique_metric_names.insert(metric_name), "Duplicated metric name: {}", metric_name,);
    }
}
