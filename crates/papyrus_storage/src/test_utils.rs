#![allow(clippy::unwrap_used)]
//! Test utilities for the storage crate users.

use starknet_api::core::ChainId;
use tempfile::{tempdir, TempDir};

use crate::db::DbConfig;
use crate::mmap_file::MmapFileConfig;
use crate::{open_storage, StorageConfig, StorageReader, StorageScope, StorageWriter};

/// Returns a db config and the temporary directory that holds this db.
/// The TempDir object is returned as a handler for the lifetime of this object (the temp
/// directory), thus make sure the directory won't be destroyed. The caller should propagate the
/// TempDir object until it is no longer needed. When the TempDir object is dropped, the directory
/// is deleted.
pub(crate) fn get_test_config(storage_scope: Option<StorageScope>) -> (StorageConfig, TempDir) {
    let storage_scope = storage_scope.unwrap_or_default();
    let dir = tempdir().unwrap();
    println!("{dir:?}");
    (
        StorageConfig {
            db_config: DbConfig {
                path_prefix: dir.path().to_path_buf(),
                chain_id: ChainId::Other("CHAIN_ID_SUBDIR".to_owned()),
                enforce_file_exists: false,
                min_size: 1 << 20,    // 1MB
                max_size: 1 << 35,    // 32GB
                growth_step: 1 << 26, // 64MB
            },
            scope: storage_scope,
            mmap_file_config: get_mmap_file_test_config(),
        },
        dir,
    )
}

/// Returns [`StorageReader`], [`StorageWriter`] and the temporary directory that holds a db for
/// testing purposes.
pub fn get_test_storage() -> ((StorageReader, StorageWriter), TempDir) {
    let (config, temp_dir) = get_test_config(None);
    ((open_storage(config).unwrap()), temp_dir)
}

/// Returns a [`MmapFileConfig`] for testing purposes.
pub fn get_mmap_file_test_config() -> MmapFileConfig {
    MmapFileConfig {
        max_size: 1 << 24,        // 16MB
        growth_step: 1 << 20,     // 1MB
        max_object_size: 1 << 16, // 64KB
    }
}

/// Returns [`StorageReader`], [`StorageWriter`] that configured by the given [`StorageScope`] and
/// the temporary directory that holds a db for testing purposes.
pub fn get_test_storage_by_scope(
    storage_scope: StorageScope,
) -> ((StorageReader, StorageWriter), TempDir) {
    let ((reader, writer), _config, temp_dir) =
        get_test_storage_with_config_by_scope(storage_scope);
    ((reader, writer), temp_dir)
}

/// Returns [`StorageReader`], [`StorageWriter`] that configured by the given [`StorageScope`] and
/// the temporary directory that holds a db for testing purposes. The Returned [`StorageConfig`] can
/// be used to open the exact same storage again (same DB file).
pub fn get_test_storage_with_config_by_scope(
    scope: StorageScope,
) -> ((StorageReader, StorageWriter), StorageConfig, TempDir) {
    let (mut config, temp_dir) = get_test_config(Some(scope));
    let (reader, writer) = open_storage(config.clone()).unwrap();
    config.db_config.path_prefix = temp_dir.path().to_path_buf();
    config.scope = scope;

    ((reader, writer), config, temp_dir)
}

// TODO: Make all previous functions work with the builder.
/// A tool for creating a test storage.
pub struct TestStorageBuilder {
    config: StorageConfig,
    handle: TempDir,
}

impl TestStorageBuilder {
    /// Sets the storage scope.
    pub fn scope(mut self, scope: StorageScope) -> Self {
        self.config.scope = scope;
        self
    }

    /// Sets the chain id.
    pub fn chain_id(mut self, chain_id: ChainId) -> Self {
        self.config.db_config.chain_id = chain_id;
        self
    }

    /// Finishes the building and returns [`StorageReader`], [`StorageWriter`] and [`StorageConfig`]
    /// that were built, and the temporary directory that holds a db for testing purposes. The
    /// Returned [`StorageConfig`] can be used to open the exact same storage again (same DB
    /// file).
    pub fn build(self) -> ((StorageReader, StorageWriter), StorageConfig, TempDir) {
        let (reader, writer) = open_storage(self.config.clone()).unwrap();
        ((reader, writer), self.config, self.handle)
    }
}

impl Default for TestStorageBuilder {
    fn default() -> Self {
        let (config, handle) = get_test_config(None);
        Self { config, handle }
    }
}
