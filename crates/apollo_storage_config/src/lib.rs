pub mod db;
pub mod mmap_file;
pub mod storage_reader_server;

use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::db::DbConfig;
use crate::mmap_file::MmapFileConfig;

/// The categories of data to save in the storage.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum StorageScope {
    /// Stores all types of data.
    #[default]
    FullArchive,
    /// Stores the data describing the current state. In this mode the transaction, events and
    /// state-diffs are not stored.
    StateOnly,
}

/// Configuration for transaction batching.
///
/// # When to Use Batching
///
/// Batching is designed for high-throughput sync operations where:
/// - Blocks are validated before writing (no duplicate keys, correct markers, etc.)
/// - Write operations are expected to succeed
/// - Data integrity is ensured at a higher level (by the sync protocol)
///
/// # When not to Use Batching
///
/// Do not enable batching when:
/// - Testing error handling scenarios (intentionally triggering write failures)
/// - Operations may fail partway through (e.g., duplicate key errors, marker mismatches)
/// - Immediate commit guarantees are required after each write
/// - During initialization/revert operations (batching should be disabled)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct BatchConfig {
    /// Whether batching is enabled.
    pub enabled: bool,
    /// Number of logical commits before actual MDBX commit. Must be at least 1.
    pub batch_size: NonZeroUsize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self { enabled: false, batch_size: NonZeroUsize::new(100).expect("100 is non-zero") }
    }
}

/// A struct for the configuration of the storage.
#[allow(missing_docs)]
#[derive(Serialize, Debug, Default, Deserialize, Clone, PartialEq, Validate)]
pub struct StorageConfig {
    #[validate(nested)]
    pub db_config: DbConfig,
    #[validate(nested)]
    pub mmap_file_config: MmapFileConfig,
    pub scope: StorageScope,
    #[serde(default)]
    #[validate(nested)]
    pub batch_config: BatchConfig,
}
