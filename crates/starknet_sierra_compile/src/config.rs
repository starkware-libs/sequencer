use std::collections::BTreeMap;
#[cfg(feature = "cairo_native")]
use std::path::PathBuf;

#[cfg(feature = "cairo_native")]
use itertools::chain;
#[cfg(feature = "cairo_native")]
use papyrus_config::dumping::ser_optional_param;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_CASM_BYTECODE_SIZE: usize = 81920;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// Bytecode size limit.
    pub max_casm_bytecode_size: usize,
    #[cfg(feature = "cairo_native")]
    /// Sierra-to-Native compiler binary path.
    pub sierra_to_native_compiler_path: Option<PathBuf>,
    #[cfg(feature = "cairo_native")]
    /// Path to Cairo native runtime library file.
    pub libcairo_native_runtime_path: Option<PathBuf>,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_casm_bytecode_size: DEFAULT_MAX_CASM_BYTECODE_SIZE,
            #[cfg(feature = "cairo_native")]
            sierra_to_native_compiler_path: None,
            #[cfg(feature = "cairo_native")]
            libcairo_native_runtime_path: None,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let max_casm_bytecode_size_dump = BTreeMap::from_iter([ser_param(
            "max_casm_bytecode_size",
            &self.max_casm_bytecode_size,
            "Limitation of compiled casm bytecode size.",
            ParamPrivacyInput::Public,
        )]);
        #[cfg(feature = "cairo_native")]
        return chain!(
            max_casm_bytecode_size_dump,
            ser_optional_param(
                &self.sierra_to_native_compiler_path,
                "".into(),
                "sierra_to_native_compiler_path",
                "The path to the Sierra-to-Native compiler binary.",
                ParamPrivacyInput::Public,
            ),
            ser_optional_param(
                &self.libcairo_native_runtime_path,
                "".into(),
                "libcairo_native_runtime_path",
                "The path to the Cairo native runtime library file.",
                ParamPrivacyInput::Public,
            )
        )
        .collect();
        #[cfg(not(feature = "cairo_native"))]
        return max_casm_bytecode_size_dump;
    }
}
