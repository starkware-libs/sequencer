use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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

impl SerializeConfig for CachedClassStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "class_cache_size",
                &self.class_cache_size,
                "Contract classes cache size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "deprecated_class_cache_size",
                &self.deprecated_class_cache_size,
                "Deprecated contract classes cache size.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// Configuration for filesystem class storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct FsClassStorageConfig {
    pub persistent_root: PathBuf,
    pub class_hash_storage_config: StorageConfig,
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
            },
            storage_reader_server_static_config: StorageReaderServerStaticConfig::default(),
        }
    }
}

impl SerializeConfig for FsClassStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "persistent_root",
            &self.persistent_root,
            "Path to the node's class storage directory.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut prepend_sub_config_name(
            self.class_hash_storage_config.dump(),
            "class_hash_storage_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.storage_reader_server_static_config.dump(),
            "storage_reader_server_static_config",
        ));
        dump
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

impl SerializeConfig for ClassManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "max_compiled_contract_class_object_size",
            &self.max_compiled_contract_class_object_size,
            "Limitation of compiled contract class object size.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut prepend_sub_config_name(
            self.cached_class_storage_config.dump(),
            "cached_class_storage_config",
        ));
        dump
    }
}

/// Dynamic configuration for class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerDynamicConfig {
    pub storage_reader_server_dynamic_config: StorageReaderServerDynamicConfig,
}

impl SerializeConfig for ClassManagerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        prepend_sub_config_name(
            self.storage_reader_server_dynamic_config.dump(),
            "storage_reader_server_dynamic_config",
        )
    }
}

/// Static configuration for filesystem-based class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerStaticConfig {
    pub class_manager_config: ClassManagerConfig,
    pub class_storage_config: FsClassStorageConfig,
}

impl SerializeConfig for ClassManagerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::new();
        dump.append(&mut prepend_sub_config_name(
            self.class_manager_config.dump(),
            "class_manager_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.class_storage_config.dump(),
            "class_storage_config",
        ));
        dump
    }
}

/// Configuration for filesystem-based class manager.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FsClassManagerConfig {
    #[validate(nested)]
    pub static_config: ClassManagerStaticConfig,
    #[validate(nested)]
    pub dynamic_config: ClassManagerDynamicConfig,
}

impl SerializeConfig for FsClassManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config
    }
}
