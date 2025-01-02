use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

// FIXME: The "faulty account" contract compiled in the test reaches above 12 MB in compiled
// native.
pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 20 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 15;
// FIXME: The test fails if the memory limit is <= 1100 MB.
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 1110 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraToCasmCompilationConfig {
    /// CASM bytecode size limit.
    pub max_bytecode_size: usize,
}

impl Default for SierraToCasmCompilationConfig {
    fn default() -> Self {
        Self { max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE }
    }
}

impl SerializeConfig for SierraToCasmCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "max_bytecode_size",
            &self.max_bytecode_size,
            "Limitation of contract bytecode size.",
            ParamPrivacyInput::Public,
        )])
    }
}
