use std::collections::BTreeMap;

use apollo_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: Option<u64>,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
            max_memory_usage: Some(DEFAULT_MAX_MEMORY_USAGE),
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "max_bytecode_size",
            &self.max_bytecode_size,
            "Limitation of compiled CASM bytecode size (felts).",
            ParamPrivacyInput::Public,
        )]);
        dump.extend(ser_optional_param(
            &self.max_memory_usage,
            DEFAULT_MAX_MEMORY_USAGE,
            "max_memory_usage",
            "Limitation of compilation process's virtual memory (bytes).",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}
