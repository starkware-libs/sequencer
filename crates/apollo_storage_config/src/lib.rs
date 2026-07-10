pub mod db;
pub mod mmap_file;
pub mod storage_reader_server;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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

impl SerializeConfig for BatchConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "enabled",
                &self.enabled,
                "Whether transaction batching is enabled.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "batch_size",
                &self.batch_size,
                "Number of logical commits before actual MDBX commit.",
                ParamPrivacyInput::Public,
            ),
        ])
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

impl SerializeConfig for StorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dumped_config = BTreeMap::from_iter([ser_param(
            "scope",
            &self.scope,
            "The categories of data saved in storage.",
            ParamPrivacyInput::Public,
        )]);
        dumped_config
            .extend(prepend_sub_config_name(self.mmap_file_config.dump(), "mmap_file_config"));
        dumped_config.extend(prepend_sub_config_name(self.db_config.dump(), "db_config"));
        dumped_config.extend(prepend_sub_config_name(self.batch_config.dump(), "batch_config"));
        dumped_config
    }
}
