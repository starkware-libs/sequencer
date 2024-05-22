use std::collections::BTreeMap;
use std::net::IpAddr;

use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    pub network_config: GatewayNetworkConfig,
    pub stateless_transaction_validator_config: StatelessTransactionValidatorConfig,
    // TODO(Arni): Add the stateful transaction validator config.
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.network_config.dump(), "network_config"),
            append_sub_config_name(
                self.stateless_transaction_validator_config.dump(),
                "stateless_transaction_validator_config",
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// The gateway network connection related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayNetworkConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl SerializeConfig for GatewayNetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The gateway server ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param("port", &self.port, "The gateway server port.", ParamPrivacyInput::Public),
        ])
    }
}

impl Default for GatewayNetworkConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8080 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatelessTransactionValidatorConfig {
    // If true, validates that the resource bounds are not zero.
    pub validate_non_zero_l1_gas_fee: bool,
    pub validate_non_zero_l2_gas_fee: bool,

    pub max_calldata_length: usize,
    pub max_signature_length: usize,
}

impl SerializeConfig for StatelessTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "validate_non_zero_l1_gas_fee",
                &self.validate_non_zero_l1_gas_fee,
                "If true, validates that a transaction has non-zero L1 resource bounds.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_non_zero_l2_gas_fee",
                &self.validate_non_zero_l2_gas_fee,
                "If true, validates that a transaction has non-zero L2 resource bounds.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_signature_length",
                &self.max_signature_length,
                "Validates that a transaction has calldata length less than or equal to this \
                 value.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_calldata_length",
                &self.max_calldata_length,
                "Validates that a transaction has signature length less than or equal to this \
                 value.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct RpcStateReaderConfig {
    pub url: String,
    pub json_rpc_version: String,
}

#[cfg(any(feature = "testing", test))]
impl RpcStateReaderConfig {
    pub fn create_for_testing() -> Self {
        Self { url: "http://localhost:8080".to_string(), json_rpc_version: "2.0".to_string() }
    }
}

impl SerializeConfig for RpcStateReaderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("url", &self.url, "The url of the rpc server.", ParamPrivacyInput::Public),
            ser_param(
                "json_rpc_version",
                &self.json_rpc_version,
                "The json rpc version.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

// TODO(Arni): Remove this struct once Chain info supports Papyrus serialization.
#[derive(Clone, Debug)]
pub struct ChainInfoConfig {
    pub chain_id: ChainId,
    pub strk_fee_token_address: ContractAddress,
    pub eth_fee_token_address: ContractAddress,
}

impl From<ChainInfoConfig> for ChainInfo {
    fn from(chain_info: ChainInfoConfig) -> Self {
        Self {
            chain_id: chain_info.chain_id,
            fee_token_addresses: FeeTokenAddresses {
                strk_fee_token_address: chain_info.strk_fee_token_address,
                eth_fee_token_address: chain_info.eth_fee_token_address,
            },
        }
    }
}

impl From<ChainInfo> for ChainInfoConfig {
    fn from(chain_info: ChainInfo) -> Self {
        let FeeTokenAddresses { strk_fee_token_address, eth_fee_token_address } =
            chain_info.fee_token_addresses;
        Self { chain_id: chain_info.chain_id, strk_fee_token_address, eth_fee_token_address }
    }
}

impl Default for ChainInfoConfig {
    fn default() -> Self {
        ChainInfo::default().into()
    }
}

impl ChainInfoConfig {
    pub fn create_for_testing() -> Self {
        BlockContext::create_for_testing().chain_info().clone().into()
    }
}

#[derive(Clone, Debug, Default)]
pub struct StatefulTransactionValidatorConfig {
    pub max_nonce_for_validation_skip: Nonce,
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub chain_info: ChainInfoConfig,
}

impl StatefulTransactionValidatorConfig {
    pub fn create_for_testing() -> Self {
        StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: 1000000,
            max_recursion_depth: 50,
            chain_info: ChainInfoConfig::create_for_testing(),
        }
    }
}
