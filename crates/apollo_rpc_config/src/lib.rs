use std::net::{IpAddr, Ipv4Addr};

use apollo_config::retry::RetryConfig;
use apollo_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::fee_token_defaults::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use validator::Validate;

pub const RPC_CONFIG_DEFAULT_PORT: u16 = 8090;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
pub struct RpcConfig {
    #[validate(custom(function = "validate_ascii"))]
    pub chain_id: ChainId,
    pub ip: IpAddr,
    pub port: u16,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    // TODO(lev,shahak): remove once we remove papyrus.
    pub collect_metrics: bool,
    pub starknet_url: String,
    pub apollo_gateway_retry_config: RetryConfig,
    pub execution_config: ExecutionConfig,
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig {
            chain_id: ChainId::Mainnet,
            ip: Ipv4Addr::UNSPECIFIED.into(),
            port: RPC_CONFIG_DEFAULT_PORT,
            max_events_chunk_size: 1000,
            max_events_keys: 100,
            collect_metrics: false,
            starknet_url: String::from("https://alpha-mainnet.starknet.io/"),
            apollo_gateway_retry_config: RetryConfig {
                retry_base_millis: 50,
                retry_max_delay_millis: 1000,
                max_retries: 5,
            },
            execution_config: ExecutionConfig::default(),
        }
    }
}

const DEFAULT_INITIAL_GAS_COST: u64 = 10000000000;

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq)]
/// Parameters that are needed for execution.
pub struct ExecutionConfig {
    /// The strk address to receive fees
    pub strk_fee_contract_address: ContractAddress,
    /// The eth address to receive fees
    pub eth_fee_contract_address: ContractAddress,
    /// The initial gas cost for a transaction
    pub default_initial_gas_cost: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        ExecutionConfig {
            strk_fee_contract_address: *STRK_FEE_CONTRACT_ADDRESS,
            eth_fee_contract_address: *ETH_FEE_CONTRACT_ADDRESS,
            default_initial_gas_cost: DEFAULT_INITIAL_GAS_COST,
        }
    }
}
