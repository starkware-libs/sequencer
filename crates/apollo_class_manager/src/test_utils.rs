use std::path::PathBuf;

use starknet_api::core::ChainId;
use tempfile::TempDir;

use crate::class_storage::{ClassHashStorage, FsClassStorage};
use crate::config::{ClassHashStorageConfig, FsClassStorageConfig};

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
        let config = FsClassStorageConfig {
            persistent_root,
            class_hash_storage_config: ClassHashStorageConfig {
                path_prefix: class_hash_storage_handle.path().to_path_buf(),
                enforce_file_exists: false,
                max_size: 1 << 35, // 32GB.
                chain_id: ChainId::Other("UnusedChainID".to_string()),
            },
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
        self.config.class_hash_storage_config.path_prefix = class_hash_storage_path_prefix;
        self.config.persistent_root = persistent_path;
        self.handles = None;
        self
    }

    pub fn build(self) -> (FsClassStorage, FsClassStorageConfig, Option<FileHandles>) {
        let Self { config, handles } = self;
        let class_hash_storage =
            ClassHashStorage::new(config.class_hash_storage_config.clone()).unwrap();
        let fs_class_storage =
            FsClassStorage { persistent_root: config.persistent_root.clone(), class_hash_storage };
        (fs_class_storage, config, handles)
    }
}
