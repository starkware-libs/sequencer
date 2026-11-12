use std::path::PathBuf;

use apollo_config::validators::validate_ascii;
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
