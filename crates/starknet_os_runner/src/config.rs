//! Configuration for the Starknet OS Runner.

use blockifier::blockifier::config::ContractClassManagerConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};

use crate::runner::RunnerConfig;

/// Configuration for the VirtualSnosProver.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ProverConfig {
    /// Configuration for the contract class manager.
    pub contract_class_manager_config: ContractClassManagerConfig,
    /// Chain ID of the network.
    pub chain_id: ChainId,
    /// RPC node URL for fetching state.
    pub rpc_node_url: String,
    /// Configuration for the runner.
    pub runner_config: RunnerConfig,
    /// Optional override for the STRK fee token address (e.g., for custom environments).
    pub strk_fee_token_address: Option<ContractAddress>,
}

impl Default for ProverConfig {
    fn default() -> Self {
        Self {
            contract_class_manager_config: ContractClassManagerConfig::default(),
            chain_id: ChainId::Mainnet,
            rpc_node_url: String::new(),
            runner_config: RunnerConfig::default(),
            strk_fee_token_address: None,
        }
    }
}
