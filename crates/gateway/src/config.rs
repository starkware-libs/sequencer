use std::collections::BTreeMap;
use std::net::IpAddr;

use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::compiler_version::VersionId;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    pub network_config: GatewayNetworkConfig,
    pub stateless_tx_validator_config: StatelessTransactionValidatorConfig,
    pub stateful_tx_validator_config: StatefulTransactionValidatorConfig,
    pub compiler_config: GatewayCompilerConfig,
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.network_config.dump(), "network_config"),
            append_sub_config_name(
                self.stateless_tx_validator_config.dump(),
                "stateless_tx_validator_config",
            ),
            append_sub_config_name(
                self.stateful_tx_validator_config.dump(),
                "stateful_tx_validator_config",
            ),
            append_sub_config_name(self.compiler_config.dump(), "compiler_config"),
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatelessTransactionValidatorConfig {
    // If true, validates that the resource bounds are not zero.
    pub validate_non_zero_l1_gas_fee: bool,
    pub validate_non_zero_l2_gas_fee: bool,
    pub max_calldata_length: usize,
    pub max_signature_length: usize,

    // Declare txs specific config.
    pub max_bytecode_size: usize,
    pub max_raw_class_size: usize,
    pub min_sierra_version: VersionId,
    pub max_sierra_version: VersionId,
}

impl Default for StatelessTransactionValidatorConfig {
    fn default() -> Self {
        StatelessTransactionValidatorConfig {
            validate_non_zero_l1_gas_fee: true,
            validate_non_zero_l2_gas_fee: false,
            max_calldata_length: 4000,
            max_signature_length: 4000,
            max_bytecode_size: 81920,
            max_raw_class_size: 4089446,
            min_sierra_version: VersionId::new(1, 1, 0),
            max_sierra_version: VersionId::new(1, 5, usize::MAX),
        }
    }
}

impl SerializeConfig for StatelessTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([
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
                "Limitation of signature length.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_calldata_length",
                &self.max_calldata_length,
                "Limitation of calldata length.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_bytecode_size",
                &self.max_bytecode_size,
                "Limitation of contract bytecode size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_raw_class_size",
                &self.max_raw_class_size,
                "Limitation of contract class object size.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![
            members,
            append_sub_config_name(self.min_sierra_version.dump(), "min_sierra_version"),
            append_sub_config_name(self.max_sierra_version.dump(), "max_sierra_version"),
        ]
        .into_iter()
        .flatten()
        .collect()
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
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
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
    #[cfg(any(test, feature = "testing"))]
    pub fn create_for_testing() -> Self {
        BlockContext::create_for_testing().chain_info().clone().into()
    }
}

impl SerializeConfig for ChainInfoConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain ID of the StarkNet chain.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "strk_fee_token_address",
                &self.strk_fee_token_address,
                "Address of the STRK fee token.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "eth_fee_token_address",
                &self.eth_fee_token_address,
                "Address of the ETH fee token.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatefulTransactionValidatorConfig {
    pub max_nonce_for_validation_skip: Nonce,
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub chain_info: ChainInfoConfig,
}

impl Default for StatefulTransactionValidatorConfig {
    fn default() -> Self {
        StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Nonce(Felt::ONE),
            validate_max_n_steps: 1_000_000,
            max_recursion_depth: 50,
            chain_info: ChainInfoConfig::default(),
        }
    }
}

impl SerializeConfig for StatefulTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([
            ser_param(
                "max_nonce_for_validation_skip",
                &self.max_nonce_for_validation_skip,
                "Maximum nonce for which the validation is skipped.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_max_n_steps",
                &self.validate_max_n_steps,
                "Maximum number of steps the validation function is allowed to take.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_recursion_depth",
                &self.max_recursion_depth,
                "Maximum recursion depth for nested calls during blockifier validation.",
                ParamPrivacyInput::Public,
            ),
        ]);
        let sub_configs = append_sub_config_name(self.chain_info.dump(), "chain_info");
        vec![members, sub_configs].into_iter().flatten().collect()
    }
}

impl StatefulTransactionValidatorConfig {
    #[cfg(any(test, feature = "testing"))]
    pub fn create_for_testing() -> Self {
        StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: 1000000,
            max_recursion_depth: 50,
            chain_info: ChainInfoConfig::create_for_testing(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayCompilerConfig {}

impl SerializeConfig for GatewayCompilerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}
