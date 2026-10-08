use std::collections::BTreeMap;
use std::result;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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

impl SerializeConfig for MmapFileConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of a memory mapped file in bytes. Must be greater than \
                 growth_step.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "growth_step",
                &self.growth_step,
                "The growth step in bytes, must be greater than max_object_size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_object_size",
                &self.max_object_size,
                "The maximum size of a single object in the file in bytes",
                ParamPrivacyInput::Public,
            ),
        ])
    }
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
