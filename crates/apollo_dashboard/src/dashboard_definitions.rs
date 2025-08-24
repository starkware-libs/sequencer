use apollo_batcher::metrics::BATCHER_INFRA_METRICS;
use apollo_class_manager::metrics::CLASS_MANAGER_INFRA_METRICS;
use apollo_compile_to_casm::metrics::SIERRA_COMPILER_INFRA_METRICS;
use apollo_gateway::metrics::GATEWAY_INFRA_METRICS;
use apollo_l1_endpoint_monitor_types::L1_ENDPOINT_MONITOR_INFRA_METRICS;
use apollo_l1_gas_price::metrics::L1_GAS_PRICE_INFRA_METRICS;
use apollo_l1_provider::metrics::L1_PROVIDER_INFRA_METRICS;
use apollo_mempool::metrics::MEMPOOL_INFRA_METRICS;
use apollo_mempool_p2p::metrics::MEMPOOL_P2P_INFRA_METRICS;
use apollo_state_sync_metrics::metrics::STATE_SYNC_INFRA_METRICS;

use crate::dashboard::{get_component_infra_row, Dashboard};
use crate::panels::batcher::get_batcher_row;
use crate::panels::blockifier::get_blockifier_row;
use crate::panels::consensus::{get_consensus_p2p_row, get_consensus_row};
use crate::panels::gateway::get_gateway_row;
use crate::panels::http_server::get_http_server_row;
use crate::panels::l1_gas_price::get_l1_gas_price_row;
use crate::panels::l1_provider::get_l1_provider_row;
use crate::panels::mempool::get_mempool_row;
use crate::panels::mempool_p2p::get_mempool_p2p_row;
use crate::panels::sierra_compiler::get_compile_to_casm_row;
use crate::panels::state_sync::{get_state_sync_p2p_row, get_state_sync_row};
use crate::panels::storage::get_storage_row;

#[cfg(test)]
#[path = "dashboard_definitions_test.rs"]
mod dashboard_definitions_test;

pub const DEV_JSON_PATH: &str = "crates/apollo_dashboard/resources/dev_grafana.json";

pub fn get_apollo_dashboard() -> Dashboard {
    Dashboard::new(
        "Sequencer Node Dashboard",
        vec![
            get_consensus_row(),
            get_batcher_row(),
            get_state_sync_row(),
            get_http_server_row(),
            get_gateway_row(),
            get_mempool_row(),
            get_l1_provider_row(),
            get_l1_gas_price_row(),
            get_blockifier_row(),
            get_compile_to_casm_row(),
            get_consensus_p2p_row(),
            get_state_sync_p2p_row(),
            get_storage_row(),
            get_mempool_p2p_row(),
            get_component_infra_row("Batcher Infra", &BATCHER_INFRA_METRICS),
            get_component_infra_row("State Sync Infra", &STATE_SYNC_INFRA_METRICS),
            get_component_infra_row("Gateway Infra", &GATEWAY_INFRA_METRICS),
            get_component_infra_row("Mempool Infra", &MEMPOOL_INFRA_METRICS),
            get_component_infra_row("Mempool P2P Infra", &MEMPOOL_P2P_INFRA_METRICS),
            get_component_infra_row("L1 Provider Infra", &L1_PROVIDER_INFRA_METRICS),
            get_component_infra_row("L1 Gas Price Infra", &L1_GAS_PRICE_INFRA_METRICS),
            get_component_infra_row("Class Manager Infra", &CLASS_MANAGER_INFRA_METRICS),
            get_component_infra_row("Sierra Compiler Infra", &SIERRA_COMPILER_INFRA_METRICS),
            get_component_infra_row(
                "L1 Endpoint Monitor Infra",
                &L1_ENDPOINT_MONITOR_INFRA_METRICS,
            ),
        ],
    )
}
