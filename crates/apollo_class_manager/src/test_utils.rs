use std::path::PathBuf;
use std::sync::Arc;

use apollo_class_manager_config::config::FsClassStorageConfig;
use apollo_storage::db::DbConfig;
use apollo_storage::storage_reader_server::{
    DynamicConfigError,
    DynamicConfigProvider,
    ServerConfig,
    StorageReaderServerDynamicConfig,
};
use async_trait::async_trait;
use starknet_api::core::ChainId;
use tempfile::TempDir;

use crate::class_storage::{ClassHashStorage, FsClassStorage};

pub type FileHandles = (TempDir, TempDir);

/// Mock provider for testing, always returns enabled=true
pub struct MockTestDynamicConfigProvider;

#[async_trait]
impl DynamicConfigProvider for MockTestDynamicConfigProvider {
    async fn get_storage_reader_dynamic_config(
        &self,
    ) -> Result<StorageReaderServerDynamicConfig, DynamicConfigError> {
        Ok(StorageReaderServerDynamicConfig { enable: true })
    }
}

pub struct FsClassStorageBuilderForTesting {
    config: FsClassStorageConfig,
    handles: Option<FileHandles>,
}

impl Default for FsClassStorageBuilderForTesting {
    fn default() -> Self {
        let class_hash_storage_handle = tempfile::tempdir().unwrap();
        let persistent_root_handle = tempfile::tempdir().unwrap();
        let persistent_root = persistent_root_handle.path().to_path_buf();
        let mut config = FsClassStorageConfig { persistent_root, ..Default::default() };
        config.class_hash_storage_config.db_config = DbConfig {
            path_prefix: class_hash_storage_handle.path().to_path_buf(),
            max_size: 1 << 30,    // 1GB.
            min_size: 1 << 10,    // 1KB.
            growth_step: 1 << 26, // 64MB.
            ..Default::default()
        };
        Self { config, handles: Some((class_hash_storage_handle, persistent_root_handle)) }
    }
}

impl FsClassStorageBuilderForTesting {
    pub fn with_existing_paths(
        mut self,
        class_hash_storage_path_prefix: PathBuf,
        persistent_path: PathBuf,
    ) -> Self {
        self.config.class_hash_storage_config.db_config.path_prefix =
            class_hash_storage_path_prefix;
        self.config.persistent_root = persistent_path;
        self.handles = None;
        self
    }

    pub fn with_chain_id(mut self, chain_id: ChainId) -> Self {
        self.config.class_hash_storage_config.db_config.chain_id = chain_id;
        self
    }

    pub fn with_storage_reader_server_port(mut self, port: u16) -> Self {
        self.config.storage_reader_server_static_config.port = port;
        self
    }

    pub fn build(self) -> (FsClassStorage, FsClassStorageConfig, Option<FileHandles>) {
        let Self { config, handles } = self;
        let dynamic_config_provider = Arc::new(MockTestDynamicConfigProvider);
        let class_hash_storage = ClassHashStorage::new(
            config.class_hash_storage_config.clone(),
            ServerConfig {
                static_config: config.storage_reader_server_static_config.clone(),
                dynamic_config: StorageReaderServerDynamicConfig::default(),
            },
            dynamic_config_provider,
        )
        .unwrap();
        let fs_class_storage =
            FsClassStorage { persistent_root: config.persistent_root.clone(), class_hash_storage };
        (fs_class_storage, config, handles)
    }
}
