use crate::dashboard::Dashboard;
use crate::panels::batcher::{get_batcher_infra_row, get_batcher_row};
use crate::panels::blockifier::get_blockifier_row;
use crate::panels::class_manager::get_class_manager_infra_row;
use crate::panels::consensus::{get_consensus_p2p_row, get_consensus_row};
use crate::panels::gateway::{get_gateway_infra_row, get_gateway_row};
use crate::panels::http_server::get_http_server_row;
use crate::panels::l1_gas_price::{get_l1_gas_price_infra_row, get_l1_gas_price_row};
use crate::panels::l1_provider::{get_l1_provider_infra_row, get_l1_provider_row};
use crate::panels::mempool::{get_mempool_infra_row, get_mempool_row};
use crate::panels::mempool_p2p::{get_mempool_p2p_infra_row, get_mempool_p2p_row};
use crate::panels::sierra_compiler::{get_compile_to_casm_row, get_sierra_compiler_infra_row};
use crate::panels::state_sync::{
    get_state_sync_infra_row,
    get_state_sync_p2p_row,
    get_state_sync_row,
};

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
            get_mempool_p2p_row(),
            get_batcher_infra_row(),
            get_state_sync_infra_row(),
            get_gateway_infra_row(),
            get_mempool_infra_row(),
            get_mempool_p2p_infra_row(),
            get_l1_provider_infra_row(),
            get_l1_gas_price_infra_row(),
            get_class_manager_infra_row(),
            get_sierra_compiler_infra_row(),
        ],
    )
}
