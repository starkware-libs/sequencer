use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;
// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 15 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 20;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
    /// File size limit (in bytes).
    pub max_file_size: u64,
    /// Compilation CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_bytecode_size",
                &self.max_bytecode_size,
                "Limitation of compiled bytecode size (felts).",
                ParamPrivacyInput::Public,
            ),
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
        ])
    }
}
