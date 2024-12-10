use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraToCasmCompilationConfig {
    /// CASM bytecode size limit.
    pub max_bytecode_size: usize,
}

impl Default for SierraToCasmCompilationConfig {
    fn default() -> Self {
        Self { max_bytecode_size: 81920 }
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
