use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::{Validate, ValidationError};

/// The configuration of the database.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
#[validate(schema(function = "validate_db_config"))]
pub struct DbConfig {
    /// The path prefix of the database files. The final path is the path prefix followed by the
    /// chain id.
    pub path_prefix: PathBuf,
    /// The [chain id](https://docs.rs/starknet_api/latest/starknet_api/core/struct.ChainId.html) of the Starknet network.
    #[validate(custom(function = "validate_ascii"))]
    pub chain_id: ChainId,
    /// Whether to enforce that the path exists. If true, `open_env` fails when the mdbx.dat file
    /// does not exist.
    pub enforce_file_exists: bool,
    /// The minimum size of the database.
    pub min_size: usize,
    /// The maximum size of the database.
    pub max_size: usize,
    /// The growth step of the database.
    pub growth_step: isize,
    /// The maximum number of readers used by the database.
    pub max_readers: u32,
}

impl Default for DbConfig {
    fn default() -> Self {
        DbConfig {
            path_prefix: PathBuf::from("./data"),
            // TODO(guyn): should we remove the default for chain_id?
            chain_id: ChainId::Mainnet,
            enforce_file_exists: false,
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 40,    // 1TB
            growth_step: 1 << 32, // 4GB
            max_readers: 1 << 13, // 8K readers
        }
    }
}

impl SerializeConfig for DbConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "path_prefix",
                &self.path_prefix,
                "Prefix of the path of the node's storage directory, the storage file path \
                will be <path_prefix>/<chain_id>. The path is not created automatically.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/learn/cheatsheets/transactions-reference#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enforce_file_exists",
                &self.enforce_file_exists,
                "Whether to enforce that the path exists. If true, `open_env` fails when the \
                mdbx.dat file does not exist.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_size",
                &self.min_size,
                "The minimum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "growth_step",
                &self.growth_step,
                "The growth step in bytes, must be greater than zero to allow the database to \
                 grow.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_readers",
                &self.max_readers,
                "The maximum number of readers used by the database.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl DbConfig {
    /// Returns the path of the database (path prefix, followed by the chain id).
    pub fn path(&self) -> PathBuf {
        self.path_prefix.join(self.chain_id.to_string().as_str())
    }
}

fn validate_db_config(config: &DbConfig) -> Result<(), ValidationError> {
    if config.min_size == 0 {
        return Err(ValidationError::new("min_size must be greater than zero"));
    }
    if config.min_size > config.max_size {
        return Err(ValidationError::new("min_size must be less than or equal to max_size"));
    }
    if config.growth_step <= 0 {
        return Err(ValidationError::new("growth_step must be greater than zero"));
    }
    if isize::try_from(config.max_size).is_err() {
        return Err(ValidationError::new("max_size exceeds isize::MAX"));
    }
    Ok(())
}
