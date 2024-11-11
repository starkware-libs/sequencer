use std::collections::BTreeMap;

use blockifier::context::ChainInfo;
use blockifier::versioned_constants::VersionedConstantsOverrides;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::Nonce;
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::compiler_version::VersionId;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    pub stateless_tx_validator_config: StatelessTransactionValidatorConfig,
    pub stateful_tx_validator_config: StatefulTransactionValidatorConfig,
    pub chain_info: ChainInfo,
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(
                self.stateless_tx_validator_config.dump(),
                "stateless_tx_validator_config",
            ),
            append_sub_config_name(
                self.stateful_tx_validator_config.dump(),
                "stateful_tx_validator_config",
            ),
            append_sub_config_name(self.chain_info.dump(), "chain_info"),
        ]
        .into_iter()
        .flatten()
        .collect()
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
    pub max_contract_class_object_size: usize,
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
            max_contract_class_object_size: 4089446,
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
                "max_contract_class_object_size",
                &self.max_contract_class_object_size,
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatefulTransactionValidatorConfig {
    pub max_nonce_for_validation_skip: Nonce,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
}

impl Default for StatefulTransactionValidatorConfig {
    fn default() -> Self {
        StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Nonce(Felt::ONE),
            versioned_constants_overrides: VersionedConstantsOverrides {
                validate_max_n_steps: 1_000_000,
                max_recursion_depth: 50,
                ..Default::default()
            },
        }
    }
}

impl SerializeConfig for StatefulTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([ser_param(
            "max_nonce_for_validation_skip",
            &self.max_nonce_for_validation_skip,
            "Maximum nonce for which the validation is skipped.",
            ParamPrivacyInput::Public,
        )]);

        [
            members,
            append_sub_config_name(
                self.versioned_constants_overrides.dump(),
                "versioned_constants_overrides",
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
