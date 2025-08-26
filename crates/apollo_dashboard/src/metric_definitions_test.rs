use std::collections::HashSet;

use apollo_batcher::metrics::{BATCHER_ALL_METRICS, INFRA_ALL_METRICS as BATCHER_INFRA_METRICS};
use apollo_class_manager::metrics::{
    CLASS_MANAGER_ALL_METRICS,
    INFRA_ALL_METRICS as CLASS_MANAGER_INFRA_METRICS,
};
use apollo_compile_to_casm::metrics::{
    COMPILE_TO_CASM_ALL_METRICS,
    INFRA_ALL_METRICS as COMPILE_TO_CASM_INFRA_METRICS,
};
use apollo_consensus::metrics::CONSENSUS_ALL_METRICS;
use apollo_consensus_manager::metrics::CONSENSUS_MANAGER_ALL_METRICS;
use apollo_consensus_orchestrator::metrics::CONSENSUS_ORCHESTRATOR_ALL_METRICS;
use apollo_gateway::metrics::{GATEWAY_ALL_METRICS, INFRA_ALL_METRICS as GATEWAY_INFRA_METRICS};
use apollo_http_server::metrics::HTTP_SERVER_ALL_METRICS;
use apollo_l1_endpoint_monitor_types::INFRA_ALL_METRICS as L1_ENDPOINT_MONITOR_INFRA_METRICS;
use apollo_l1_gas_price::metrics::{
    INFRA_ALL_METRICS as L1_GAS_PRICE_INFRA_METRICS,
    L1_GAS_PRICE_ALL_METRICS,
};
use apollo_l1_provider::metrics::{
    INFRA_ALL_METRICS as L1_PROVIDER_INFRA_METRICS,
    L1_PROVIDER_ALL_METRICS,
};
use apollo_mempool::metrics::{INFRA_ALL_METRICS as MEMPOOL_INFRA_METRICS, MEMPOOL_ALL_METRICS};
use apollo_mempool_p2p::metrics::{
    INFRA_ALL_METRICS as MEMPOOL_P2P_INFRA_METRICS,
    MEMPOOL_P2P_ALL_METRICS,
};
use apollo_state_sync_metrics::metrics::{
    INFRA_ALL_METRICS as STATE_SYNC_INFRA_METRICS,
    STATE_SYNC_ALL_METRICS,
};
use blockifier::metrics::BLOCKIFIER_ALL_METRICS;

#[test]
fn metric_names_no_duplications() {
    let all_metric_names = BATCHER_ALL_METRICS
        .iter()
        .chain(BATCHER_INFRA_METRICS.iter())
        .chain(CLASS_MANAGER_ALL_METRICS.iter())
        .chain(CLASS_MANAGER_INFRA_METRICS.iter())
        .chain(COMPILE_TO_CASM_ALL_METRICS.iter())
        .chain(COMPILE_TO_CASM_INFRA_METRICS.iter())
        .chain(CONSENSUS_ALL_METRICS.iter())
        .chain(CONSENSUS_MANAGER_ALL_METRICS.iter())
        .chain(CONSENSUS_ORCHESTRATOR_ALL_METRICS.iter())
        .chain(GATEWAY_ALL_METRICS.iter())
        .chain(GATEWAY_INFRA_METRICS.iter())
        .chain(HTTP_SERVER_ALL_METRICS.iter())
        .chain(L1_ENDPOINT_MONITOR_INFRA_METRICS.iter())
        .chain(L1_GAS_PRICE_ALL_METRICS.iter())
        .chain(L1_GAS_PRICE_INFRA_METRICS.iter())
        .chain(L1_PROVIDER_ALL_METRICS.iter())
        .chain(L1_PROVIDER_INFRA_METRICS.iter())
        .chain(MEMPOOL_ALL_METRICS.iter())
        .chain(MEMPOOL_INFRA_METRICS.iter())
        .chain(MEMPOOL_P2P_ALL_METRICS.iter())
        .chain(MEMPOOL_P2P_INFRA_METRICS.iter())
        .chain(STATE_SYNC_ALL_METRICS.iter())
        .chain(STATE_SYNC_INFRA_METRICS.iter())
        .chain(BLOCKIFIER_ALL_METRICS.iter())
        .collect::<Vec<&&'static str>>();

    let mut unique_metric_names: HashSet<&&'static str> = HashSet::new();
    for metric_name in all_metric_names {
        assert!(unique_metric_names.insert(metric_name), "Duplicated metric name: {}", metric_name);
    }
}
