use std::collections::HashSet;

use apollo_central_sync::metrics::APOLLO_CENTRAL_SYNC_ALL_METRICS;
use starknet_batcher::metrics::BATCHER_ALL_METRICS;
use starknet_consensus_manager::metrics::CONSENSUS_ALL_METRICS;
use starknet_gateway::metrics::GATEWAY_ALL_METRICS;
use starknet_http_server::metrics::HTTP_SERVER_ALL_METRICS;
use starknet_mempool::metrics::MEMPOOL_ALL_METRICS;
use starknet_mempool_p2p::metrics::MEMPOOL_P2P_ALL_METRICS;
use starknet_sequencer_infra::metrics::INFRA_ALL_METRICS;
use starknet_state_sync::metrics::STATE_SYNC_ALL_METRICS;

#[test]
fn metric_names_no_duplications() {
    let all_metric_names = BATCHER_ALL_METRICS
        .iter()
        .chain(CONSENSUS_ALL_METRICS.iter())
        .chain(GATEWAY_ALL_METRICS.iter())
        .chain(HTTP_SERVER_ALL_METRICS.iter())
        .chain(INFRA_ALL_METRICS.iter())
        .chain(MEMPOOL_ALL_METRICS.iter())
        .chain(MEMPOOL_P2P_ALL_METRICS.iter())
        .chain(APOLLO_CENTRAL_SYNC_ALL_METRICS.iter())
        .chain(STATE_SYNC_ALL_METRICS.iter())
        .collect::<Vec<&&'static str>>();

    let mut unique_metric_names: HashSet<&&'static str> = HashSet::new();
    for metric_name in all_metric_names {
        assert!(unique_metric_names.insert(metric_name), "Duplicated metric name: {}", metric_name);
    }
}
