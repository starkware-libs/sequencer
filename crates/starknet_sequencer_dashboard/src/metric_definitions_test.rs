use std::collections::HashSet;

use starknet_gateway::metrics::GATEWAY_ALL_METRICS;
use starknet_sequencer_metrics::metric_definitions::{
    BATCHER_ALL_METRICS,
    HTTP_SERVER_ALL_METRICS,
    INFRA_ALL_METRICS,
    NETWORK_ALL_METRICS,
    PAPYRUS_SYNC_ALL_METRICS,
};

#[test]
fn metric_names_no_duplications() {
    let all_metric_names = BATCHER_ALL_METRICS
        .iter()
        .chain(GATEWAY_ALL_METRICS.iter())
        .chain(HTTP_SERVER_ALL_METRICS.iter())
        .chain(INFRA_ALL_METRICS.iter())
        .chain(NETWORK_ALL_METRICS.iter())
        .chain(PAPYRUS_SYNC_ALL_METRICS.iter())
        .collect::<Vec<&&'static str>>();

    let mut unique_metric_names: HashSet<&&'static str> = HashSet::new();
    for metric_name in all_metric_names {
        assert!(unique_metric_names.insert(metric_name), "Duplicated metric name: {}", metric_name);
    }
}
