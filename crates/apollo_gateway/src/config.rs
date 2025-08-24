use std::collections::BTreeMap;
use std::str::FromStr;

use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use blockifier::context::ChainInfo;
use serde::{de, Deserialize, Deserializer, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::compiler_version::VersionId;

const JSON_RPC_VERSION: &str = "2.0";

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    pub stateless_tx_validator_config: StatelessTransactionValidatorConfig,
    pub stateful_tx_validator_config: StatefulTransactionValidatorConfig,
    pub chain_info: ChainInfo,
    pub block_declare: bool,
    #[serde(default, deserialize_with = "deserialize_optional_contract_addresses")]
    pub authorized_declarer_accounts: Option<Vec<ContractAddress>>,
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([ser_param(
            "block_declare",
            &self.block_declare,
            "If true, the gateway will block declare transactions.",
            ParamPrivacyInput::Public,
        )]);
        dump.extend(prepend_sub_config_name(
            self.stateless_tx_validator_config.dump(),
            "stateless_tx_validator_config",
        ));
        dump.extend(prepend_sub_config_name(
            self.stateful_tx_validator_config.dump(),
            "stateful_tx_validator_config",
        ));
        dump.extend(prepend_sub_config_name(self.chain_info.dump(), "chain_info"));
        dump.extend(ser_optional_param(
            &self.authorized_declarer_accounts.as_ref().map(|accounts| {
                accounts.iter().map(|addr| addr.0.to_string()).collect::<Vec<_>>().join(",")
            }),
            "".to_string(),
            "authorized_declarer_accounts",
            "Authorized declarer accounts. If set, only these accounts can declare new contracts. \
             Addresses are in hex format and separated by a comma with no space.",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}

impl GatewayConfig {
    pub fn is_authorized_declarer(&self, declarer_address: &ContractAddress) -> bool {
        match &self.authorized_declarer_accounts {
            Some(allowed_accounts) => allowed_accounts.contains(declarer_address),
            None => true,
        }
    }
}

fn deserialize_optional_contract_addresses<'de, D>(
    de: D,
) -> Result<Option<Vec<ContractAddress>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = match Option::deserialize(de)? {
        Some(addresses) => addresses,
        None => return Ok(None),
    };

    if raw.is_empty() {
        return Err(de::Error::custom(
            "Empty string is not a valid input for contract addresses. The config field \
             `gateway_config.authorized_declarer_accounts.#is_none` is false and should be true \
             if you don't want to use this feature.",
        ));
    }

    let mut result = Vec::new();
    for addresses_str in raw.split(',') {
        let felt = Felt::from_str(addresses_str).map_err(|err| {
            de::Error::custom(format!("Failed to parse Felt from '{}': {}", addresses_str, err))
        })?;

        let addr = ContractAddress::try_from(felt).map_err(|err| {
            de::Error::custom(format!("Invalid contract address '{}': {}", addresses_str, err))
        })?;

        result.push(addr);
    }

    Ok(Some(result))
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatelessTransactionValidatorConfig {
    // If true, ensures that at least one resource bound (L1, L2, or L1 data) is greater than zero.
    pub validate_resource_bounds: bool,
    // TODO(AlonH): Remove this field and use the one from the versioned constants.
    pub min_gas_price: u128,
    pub max_calldata_length: usize,
    pub max_signature_length: usize,

    // Declare txs specific config.
    pub max_contract_bytecode_size: usize,
    pub max_contract_class_object_size: usize,
    pub min_sierra_version: VersionId,
    pub max_sierra_version: VersionId,
}

impl Default for StatelessTransactionValidatorConfig {
    fn default() -> Self {
        StatelessTransactionValidatorConfig {
            validate_resource_bounds: true,
            min_gas_price: 3_000_000_000,
            max_calldata_length: 4000,
            max_signature_length: 4000,
            max_contract_bytecode_size: 81920,
            max_contract_class_object_size: 4089446,
            min_sierra_version: VersionId::new(1, 1, 0),
            max_sierra_version: VersionId::new(1, 7, usize::MAX),
        }
    }
}

impl SerializeConfig for StatelessTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([
            ser_param(
                "validate_resource_bounds",
                &self.validate_resource_bounds,
                "If true, ensures that at least one resource bound (L1, L2, or L1 data) is \
                 greater than zero.",
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
                "max_contract_bytecode_size",
                &self.max_contract_bytecode_size,
                "Limitation of contract class bytecode size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_contract_class_object_size",
                &self.max_contract_class_object_size,
                "Limitation of contract class object size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_gas_price",
                &self.min_gas_price,
                "Minimum gas price for transactions.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![
            members,
            prepend_sub_config_name(self.min_sierra_version.dump(), "min_sierra_version"),
            prepend_sub_config_name(self.max_sierra_version.dump(), "max_sierra_version"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RpcStateReaderConfig {
    pub url: String,
    pub json_rpc_version: String,
}

impl RpcStateReaderConfig {
    pub fn from_url(url: String) -> Self {
        Self { url, ..Default::default() }
    }
}

impl Default for RpcStateReaderConfig {
    fn default() -> Self {
        Self { url: Default::default(), json_rpc_version: JSON_RPC_VERSION.to_string() }
    }
}

#[cfg(any(feature = "testing", test))]
impl RpcStateReaderConfig {
    pub fn create_for_testing() -> Self {
        Self::from_url("http://localhost:8080".to_string())
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
    // If true, ensures the max L2 gas price exceeds (a configurable percentage of) the base gas
    // price of the previous block.
    pub validate_resource_bounds: bool,
    pub max_allowed_nonce_gap: u32,
    pub reject_future_declare_txs: bool,
    pub max_nonce_for_validation_skip: Nonce,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
    // Minimum gas price as percentage of threshold to accept transactions.
    pub min_gas_price_percentage: u8, // E.g., 80 to require 80% of threshold.
}

impl Default for StatefulTransactionValidatorConfig {
    fn default() -> Self {
        StatefulTransactionValidatorConfig {
            validate_resource_bounds: true,
            max_allowed_nonce_gap: 50,
            reject_future_declare_txs: true,
            max_nonce_for_validation_skip: Nonce(Felt::ONE),
            min_gas_price_percentage: 100,
            versioned_constants_overrides: VersionedConstantsOverrides::default(),
        }
    }
}

impl SerializeConfig for StatefulTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "validate_resource_bounds",
                &self.validate_resource_bounds,
                "If true, ensures the max L2 gas price exceeds (a configurable percentage of) the \
                 base gas price of the previous block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_nonce_for_validation_skip",
                &self.max_nonce_for_validation_skip,
                "Maximum nonce for which the validation is skipped.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_allowed_nonce_gap",
                &self.max_allowed_nonce_gap,
                "The maximum allowed gap between the account nonce and the transaction nonce.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "reject_future_declare_txs",
                &self.reject_future_declare_txs,
                "If true, rejects declare transactions with future nonces.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_gas_price_percentage",
                &self.min_gas_price_percentage,
                "Minimum gas price as percentage of threshold to accept transactions.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.append(&mut prepend_sub_config_name(
            self.versioned_constants_overrides.dump(),
            "versioned_constants_overrides",
        ));
        dump
    }
}
