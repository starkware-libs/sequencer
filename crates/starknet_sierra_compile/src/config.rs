use std::collections::BTreeMap;
use std::path::PathBuf;

use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_CASM_BYTECODE_SIZE: usize = 81920;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit.
    pub max_casm_bytecode_size: usize,
    /// Sierra-to-Casm compiler binary path.
    pub path_to_sierra_to_casm_compiler: Option<PathBuf>,
    /// Sierra-to-Native compiler binary path.
    pub path_to_sierra_to_native_compiler: Option<PathBuf>,
    /// Path to Cairo native runtime library file.
    pub path_to_libcairo_native_runtime: Option<PathBuf>,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_casm_bytecode_size: DEFAULT_MAX_CASM_BYTECODE_SIZE,
            path_to_sierra_to_casm_compiler: None,
            path_to_sierra_to_native_compiler: None,
            path_to_libcairo_native_runtime: None,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([ser_param(
            "max_casm_bytecode_size",
            &self.max_casm_bytecode_size,
            "Limitation of compiled casm bytecode size.",
            ParamPrivacyInput::Public,
        )]);

        dump.extend(ser_optional_param(
            &self.path_to_sierra_to_casm_compiler,
            "".into(),
            "path_to_sierra_to_casm_compiler",
            "The path to the Sierra-to-Casm compiler binary.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.path_to_sierra_to_native_compiler,
            "".into(),
            "path_to_sierra_to_native_compiler",
            "The path to the Sierra-to-Native compiler binary.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.path_to_libcairo_native_runtime,
            "".into(),
            "path_to_libcairo_native_runtime",
            "The path to the Cairo native runtime library file.",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}
