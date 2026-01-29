use std::collections::BTreeMap;

use apollo_config::converters::{
    deserialize_comma_separated_str,
    serialize_optional_comma_separated,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use blockifier::context::ChainInfo;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::compiler_version::VersionId;

const DEFAULT_BUCKET_NAME: &str = "proof-archive";

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    pub stateless_tx_validator_config: StatelessTransactionValidatorConfig,
    pub stateful_tx_validator_config: StatefulTransactionValidatorConfig,
    pub contract_class_manager_config: ContractClassManagerConfig,
    pub chain_info: ChainInfo,
    pub block_declare: bool,
    #[serde(default, deserialize_with = "deserialize_comma_separated_str")]
    pub authorized_declarer_accounts: Option<Vec<ContractAddress>>,
    pub proof_archive_writer_config: ProofArchiveWriterConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            stateless_tx_validator_config: StatelessTransactionValidatorConfig::default(),
            stateful_tx_validator_config: StatefulTransactionValidatorConfig::default(),
            contract_class_manager_config: ContractClassManagerConfig {
                contract_cache_size: 300,
                ..Default::default()
            },
            chain_info: ChainInfo::default(),
            block_declare: false,
            authorized_declarer_accounts: None,
            proof_archive_writer_config: ProofArchiveWriterConfig::default(),
        }
    }
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
        dump.extend(prepend_sub_config_name(
            self.contract_class_manager_config.dump(),
            "contract_class_manager_config",
        ));
        dump.extend(prepend_sub_config_name(self.chain_info.dump(), "chain_info"));
        dump.extend(ser_optional_param(
            &serialize_optional_comma_separated(&self.authorized_declarer_accounts),
            "".to_string(),
            "authorized_declarer_accounts",
            "Authorized declarer accounts. If set, only these accounts can declare new contracts. \
             Addresses are in hex format and separated by a comma with no space.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(prepend_sub_config_name(
            self.proof_archive_writer_config.dump(),
            "proof_archive_writer_config",
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatelessTransactionValidatorConfig {
    // If true, ensures that at least one resource bound (L1, L2, or L1 data) is greater than zero.
    pub validate_resource_bounds: bool,
    // TODO(AlonH): Remove the `min_gas_price` field from this struct and use the one from the
    // versioned constants.
    pub min_gas_price: u128,
    pub max_l2_gas_amount: u64,
    pub max_calldata_length: usize,
    pub max_signature_length: usize,
    pub max_proof_size: usize,

    // Declare txs specific config.
    pub max_contract_bytecode_size: usize,
    pub max_contract_class_object_size: usize,
    pub min_sierra_version: VersionId,
    pub max_sierra_version: VersionId,

    // If true, allows transactions with non-empty proof_facts or proof fields.
    pub allow_client_side_proving: bool,
}

impl Default for StatelessTransactionValidatorConfig {
    fn default() -> Self {
        StatelessTransactionValidatorConfig {
            validate_resource_bounds: true,
            min_gas_price: 8_000_000_000,
            max_l2_gas_amount: 1_200_000_000,
            max_calldata_length: 5000,
            max_signature_length: 4000,
            max_contract_bytecode_size: 81920,
            max_contract_class_object_size: 4089446,
            min_sierra_version: VersionId::new(1, 1, 0),
            max_sierra_version: VersionId::new(1, 7, usize::MAX),
            allow_client_side_proving: false,
            // TODO(AvivG): This value is a placeholder and will be updated once the actual value is
            // determined.
            max_proof_size: 90000,
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
            ser_param(
                "max_l2_gas_amount",
                &self.max_l2_gas_amount,
                "Maximum allowed L2 gas amount for transactions.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "allow_client_side_proving",
                &self.allow_client_side_proving,
                "If true, allows transactions with non-empty proof_facts or proof fields.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_proof_size",
                &self.max_proof_size,
                "Limitation of proof size.",
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
pub struct StatefulTransactionValidatorConfig {
    // If true, ensures the max L2 gas price exceeds (a configurable percentage of) the base gas
    // price of the previous block.
    pub validate_resource_bounds: bool,
    pub max_allowed_nonce_gap: u32,
    pub reject_future_declare_txs: bool,
    pub max_nonce_for_validation_skip: Nonce,
    pub versioned_constants_overrides: Option<VersionedConstantsOverrides>,
    // Minimum gas price as percentage of threshold to accept transactions.
    pub min_gas_price_percentage: u8, // E.g., 80 to require 80% of threshold.
}

impl Default for StatefulTransactionValidatorConfig {
    fn default() -> Self {
        StatefulTransactionValidatorConfig {
            validate_resource_bounds: true,
            max_allowed_nonce_gap: 200,
            reject_future_declare_txs: true,
            max_nonce_for_validation_skip: Nonce(Felt::ONE),
            min_gas_price_percentage: 100,
            versioned_constants_overrides: None,
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
        dump.append(&mut ser_optional_sub_config(
            &self.versioned_constants_overrides,
            "versioned_constants_overrides",
        ));
        dump
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ProofArchiveWriterConfig {
    pub bucket_name: String,
}

impl Default for ProofArchiveWriterConfig {
    fn default() -> Self {
        Self { bucket_name: DEFAULT_BUCKET_NAME.to_string() }
    }
}

#[cfg(any(feature = "testing", test))]
impl ProofArchiveWriterConfig {
    pub fn create_for_testing() -> Self {
        // Use empty bucket name for tests to trigger mock proof writer.
        Self { bucket_name: String::new() }
    }
}

impl SerializeConfig for ProofArchiveWriterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "bucket_name",
            &self.bucket_name,
            "The name of the bucket to write proofs to. An empty string indicates a test \
             environment that does not connect to GCS.",
            ParamPrivacyInput::Public,
        )])
    }
}
