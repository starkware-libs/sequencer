use serde::{Deserialize, Serialize};
use validator::Validate;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 60;
pub const DEFAULT_AUDITED_LIBFUNCS_ONLY: bool = true;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
    /// Compilation process's CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// If true, compile with audited libfuncs only; if false, allow all libfuncs.
    pub audited_libfuncs_only: bool,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            audited_libfuncs_only: DEFAULT_AUDITED_LIBFUNCS_ONLY,
        }
    }
}
