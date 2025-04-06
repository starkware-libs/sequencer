use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self { max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "max_bytecode_size",
            &self.max_bytecode_size,
            "Limitation of compiled CASM bytecode size (felts).",
            ParamPrivacyInput::Public,
        )])
    }
}
