use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 60;
pub const DEFAULT_ALLOWED_LIBFUNCS_LIST: AllowedLibfuncsList = AllowedLibfuncsList::Audited;

/// Which libfuncs the Sierra-to-CASM compiler accepts.
///
/// Serialized as a single string, so it can be set through the flat deployment config.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AllowedLibfuncsList {
    /// The compiler's built-in audited list.
    Audited,
    /// The compiler's built-in list of every libfunc.
    All,
    /// The custom list this node ships
    /// (`apollo_compile_to_casm/resources/allowed_libfuncs.json`).
    Bundled,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
    /// Compilation process's CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// The libfunc list the compiler validates against.
    pub allowed_libfuncs_list: AllowedLibfuncsList,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            allowed_libfuncs_list: DEFAULT_ALLOWED_LIBFUNCS_LIST,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "max_bytecode_size",
                &self.max_bytecode_size,
                "Limitation of compiled CASM bytecode size (felts).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_cpu_time",
                &self.max_cpu_time,
                "Limitation of compilation cpu time (seconds).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "allowed_libfuncs_list",
                &self.allowed_libfuncs_list,
                "The libfunc list to validate against: 'audited', 'all', or 'bundled'.",
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
