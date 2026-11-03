use std::result;

use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

/// Configuration for a memory mapped file.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_config"))]
pub struct MmapFileConfig {
    /// The maximum size of the memory map in bytes.
    pub max_size: usize,
    /// The growth step of the corresponding file in bytes.
    pub growth_step: usize,
    /// The maximum size of an object in bytes.
    pub max_object_size: usize,
}

impl Default for MmapFileConfig {
    fn default() -> Self {
        Self {
            max_size: 1 << 40,        // 1TB
            growth_step: 1 << 30,     // 1GB
            max_object_size: 1 << 28, // 256MB
        }
    }
}

fn validate_config(config: &MmapFileConfig) -> result::Result<(), ValidationError> {
    if config.max_size < config.growth_step {
        return Err(ValidationError::new("max_size should be larger than growth_step"));
    }
    if config.growth_step < config.max_object_size {
        return Err(ValidationError::new("growth_step should be larger than max_object_size"));
    }
    Ok(())
}
