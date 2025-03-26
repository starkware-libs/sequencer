use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_CASM_BYTECODE_SIZE: usize = 80 * 1024;
// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_NATIVE_BYTECODE_SIZE: u64 = 15 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 20;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_OPTIMIZATION_LEVEL: u8 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_casm_bytecode_size: usize,
    /// Native bytecode size limit (in bytes).
    pub max_native_bytecode_size: u64,
    /// Compilation CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
    /// The level of optimization to apply during compilation.
    pub optimization_level: u8,
    pub panic_on_compilation_failure: bool,
    /// Sierra-to-Native compiler binary path.
    pub sierra_to_native_compiler_path: Option<PathBuf>,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_casm_bytecode_size: DEFAULT_MAX_CASM_BYTECODE_SIZE,
            sierra_to_native_compiler_path: None,
            max_native_bytecode_size: DEFAULT_MAX_NATIVE_BYTECODE_SIZE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            panic_on_compilation_failure: false,
            optimization_level: DEFAULT_OPTIMIZATION_LEVEL,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "max_casm_bytecode_size",
                &self.max_casm_bytecode_size,
                "Limitation of compiled casm bytecode size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_native_bytecode_size",
                &self.max_native_bytecode_size,
                "Limitation of compiled native bytecode size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_cpu_time",
                &self.max_cpu_time,
                "Limitation of compilation cpu time (seconds).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_memory_usage",
                &self.max_memory_usage,
                "Limitation of compilation process's virtual memory (bytes).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "optimization_level",
                &self.optimization_level,
                "The level of optimization to apply during compilation.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "panic_on_compilation_failure",
                &self.panic_on_compilation_failure,
                "Whether to panic on compilation failure.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend(ser_optional_param(
            &self.sierra_to_native_compiler_path,
            "".into(),
            "sierra_to_native_compiler_path",
            "The path to the Sierra-to-Native compiler binary.",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}
