//! Configuration for the Starknet transaction prover.

use blockifier::blockifier::config::ContractClassManagerConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};

use crate::running::runner::RunnerConfig;

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
    /// Whether to validate that fee-related fields (resource bounds, tip) are zero (default:
    /// true).
    pub validate_zero_fee_fields: bool,
    /// URL of the external blocking check JSON-RPC service. `None` disables the feature.
    pub blocking_check_url: Option<String>,
    /// Seconds to wait for the blocking check response after the proof is ready. 0 = no extra
    /// wait.
    pub blocking_check_timeout_secs: u64,
    /// Whether to allow the transaction when the blocking check is inconclusive (non-10000 error,
    /// timeout, network failure). `true` = fail-open (return proof), `false` = fail-close (return
    /// error 10000).
    pub blocking_check_fail_open: bool,
}

impl Default for ProverConfig {
    fn default() -> Self {
        Self {
            contract_class_manager_config: ContractClassManagerConfig::default(),
            chain_id: ChainId::Mainnet,
            rpc_node_url: String::new(),
            runner_config: RunnerConfig::default(),
            strk_fee_token_address: None,
            validate_zero_fee_fields: true,
            blocking_check_url: None,
            blocking_check_timeout_secs: 0,
            blocking_check_fail_open: true,
        }
    }
}
