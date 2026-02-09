use std::path::PathBuf;

use apollo_class_manager_config::config::FsClassStorageConfig;
use apollo_storage::db::DbConfig;
use starknet_api::core::ChainId;
use tempfile::TempDir;

use crate::class_storage::{ClassHashStorage, FsClassStorage};

pub type FileHandles = (TempDir, TempDir);

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

    pub fn build(self) -> (FsClassStorage, FsClassStorageConfig, Option<FileHandles>) {
        let Self { config, handles } = self;
        use apollo_config_manager_types::communication::MockConfigManagerClient;
        use apollo_storage::storage_reader_server::StorageReaderServerDynamicConfig;

        let mut mock_config_manager = MockConfigManagerClient::new();
        mock_config_manager
            .expect_get_storage_reader_dynamic_config_for_component()
            .returning(|_component| Ok(StorageReaderServerDynamicConfig { enable: true }));
        let config_manager_client = std::sync::Arc::new(mock_config_manager);

        let class_hash_storage = ClassHashStorage::new(
            config.class_hash_storage_config.clone(),
            apollo_storage::storage_reader_server::ServerConfig::default(),
            config_manager_client,
        )
        .unwrap();
        let fs_class_storage =
            FsClassStorage { persistent_root: config.persistent_root.clone(), class_hash_storage };
        (fs_class_storage, config, handles)
    }
}
