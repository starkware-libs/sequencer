use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::retry::RetryConfig;
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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

impl SerializeConfig for RpcConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut self_params_dump = BTreeMap::from_iter([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/learn/cheatsheets/transactions-reference#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "ip",
                &self.ip.to_string(), "The JSON RPC server ip.",
                ParamPrivacyInput::Public
            ),
            ser_param(
                "port",
                &self.port,
                "The JSON RPC server port.",
                ParamPrivacyInput::Public
            ),
            ser_param(
                "max_events_chunk_size",
                &self.max_events_chunk_size,
                "Maximum chunk size supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_events_keys",
                &self.max_events_keys,
                "Maximum number of keys supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_metrics",
                &self.collect_metrics,
                "If true, collect metrics for the rpc.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "starknet_url",
                &self.starknet_url,
                "URL for communicating with Starknet in write_api methods.",
                ParamPrivacyInput::Public,
            ),
        ]);

        self_params_dump
            .append(&mut prepend_sub_config_name(self.execution_config.dump(), "execution_config"));
        let mut retry_config_dump = prepend_sub_config_name(
            self.apollo_gateway_retry_config.dump(),
            "apollo_gateway_retry_config",
        );
        for param in retry_config_dump.values_mut() {
            param.description = format!(
                "For communicating with Starknet gateway, {}{}",
                param.description[0..1].to_lowercase(),
                &param.description[1..]
            );
        }
        self_params_dump.append(&mut retry_config_dump);
        self_params_dump
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

impl SerializeConfig for ExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "strk_fee_contract_address",
                &self.strk_fee_contract_address,
                "The strk fee token address to receive fees",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "eth_fee_contract_address",
                &self.eth_fee_contract_address,
                "The eth fee token address to receive fees",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "default_initial_gas_cost",
                &self.default_initial_gas_cost,
                "The initial gas cost for a transaction",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
