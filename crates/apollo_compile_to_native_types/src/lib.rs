//! Types and configuration for Cairo native compilation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use validator::Validate;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 600;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 15 * 1024 * 1024 * 1024;
pub const DEFAULT_OPTIMIZATION_LEVEL: u8 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// Cairo Native file size limit (in bytes).
    pub max_file_size: Option<u64>,
    /// Compilation CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// Compilation process's virtual memory (address space) byte limit.
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
            max_file_size: Some(DEFAULT_MAX_FILE_SIZE),
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            optimization_level: DEFAULT_OPTIMIZATION_LEVEL,
        }
    }
}

impl SierraCompilationConfig {
    pub fn create_for_testing() -> Self {
        Self {
            compiler_binary_path: None,
            max_file_size: Some(15 * 1024 * 1024),
            max_cpu_time: 20,
            max_memory_usage: 5 * 1024 * 1024 * 1024,
            optimization_level: 0,
        }
    }
}
