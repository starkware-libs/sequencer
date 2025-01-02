use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_CASM_BYTECODE_SIZE: usize = 80 * 1024;
pub const DEFAULT_MAX_NATIVE_BYTECODE_SIZE: u64 = 20 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 15;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 1200 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraToCasmCompilationConfig {
    /// CASM bytecode size limit.
    pub max_casm_bytecode_size: usize,
}

impl Default for SierraToCasmCompilationConfig {
    fn default() -> Self {
        Self { max_casm_bytecode_size: DEFAULT_MAX_CASM_BYTECODE_SIZE }
    }
}

impl SerializeConfig for SierraToCasmCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "max_bytecode_size",
            &self.max_casm_bytecode_size,
            "Limitation of contract bytecode size.",
            ParamPrivacyInput::Public,
        )])
    }
}
