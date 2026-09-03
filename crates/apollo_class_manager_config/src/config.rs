use std::path::PathBuf;

use apollo_storage::db::DbConfig;
use apollo_storage::mmap_file::MmapFileConfig;
use apollo_storage::storage_reader_server::{
    StorageReaderServerDynamicConfig,
    StorageReaderServerStaticConfig,
};
use apollo_storage::{StorageConfig, StorageScope};
use serde::{Deserialize, Serialize};
use validator::Validate;

const DEFAULT_MAX_COMPILED_CONTRACT_CLASS_OBJECT_SIZE: usize = 4089446;

/// Configuration for cached class storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CachedClassStorageConfig {
    pub class_cache_size: usize,
    pub deprecated_class_cache_size: usize,
}

impl Default for CachedClassStorageConfig {
    fn default() -> Self {
        Self { class_cache_size: 10, deprecated_class_cache_size: 10 }
    }
}

/// Configuration for filesystem class storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct FsClassStorageConfig {
    pub persistent_root: PathBuf,
    #[validate(nested)]
    pub class_hash_storage_config: StorageConfig,
    #[validate(nested)]
    pub storage_reader_server_static_config: StorageReaderServerStaticConfig,
}

impl Default for FsClassStorageConfig {
    fn default() -> Self {
        Self {
            persistent_root: "/data/classes".into(),
            class_hash_storage_config: StorageConfig {
                db_config: DbConfig {
                    path_prefix: "/data/class_hash_storage".into(),
                    ..Default::default()
                },
                mmap_file_config: MmapFileConfig {
                    max_size: 1 << 30,        // 1GB.
                    growth_step: 1 << 20,     // 1MB.
                    max_object_size: 1 << 10, // 1KB; a class hash is 32B.
                },
                scope: StorageScope::StateOnly,
                batch_config: Default::default(),
            },
            storage_reader_server_static_config: StorageReaderServerStaticConfig::default(),
        }
    }
}

/// Configuration for class manager.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
    pub max_compiled_contract_class_object_size: usize,
}

impl Default for ClassManagerConfig {
    fn default() -> Self {
        ClassManagerConfig {
            cached_class_storage_config: CachedClassStorageConfig::default(),
            max_compiled_contract_class_object_size:
                DEFAULT_MAX_COMPILED_CONTRACT_CLASS_OBJECT_SIZE,
        }
    }
}

/// Dynamic configuration for class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerDynamicConfig {
    #[validate(nested)]
    pub storage_reader_server_dynamic_config: StorageReaderServerDynamicConfig,
}

/// Static configuration for filesystem-based class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerStaticConfig {
    #[validate(nested)]
    pub class_manager_config: ClassManagerConfig,
    #[validate(nested)]
    pub class_storage_config: FsClassStorageConfig,
}

/// Configuration for filesystem-based class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FsClassManagerConfig {
    #[validate(nested)]
    pub static_config: ClassManagerStaticConfig,
    #[validate(nested)]
    pub dynamic_config: ClassManagerDynamicConfig,
}
