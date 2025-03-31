use std::collections::BTreeMap;
use std::path::PathBuf;

use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 15 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 20;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_OPTIMIZATION_LEVEL: u8 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// File size limit (in bytes).
    pub max_file_size: u64,
    /// Compilation CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
    /// The level of optimization to apply during compilation.
    pub optimization_level: u8,
    /// Compiler binary path.
    pub compiler_binary_path: Option<PathBuf>,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            compiler_binary_path: None,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            optimization_level: DEFAULT_OPTIMIZATION_LEVEL,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "max_file_size",
                &self.max_file_size,
                "Limitation of compiled file size (bytes).",
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
        ]);
        dump.extend(ser_optional_param(
            &self.compiler_binary_path,
            "".into(),
            "compiler_binary_path",
            "The path to the Sierra-to-Native compiler binary.",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}
