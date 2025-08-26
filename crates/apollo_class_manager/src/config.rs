use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::mmap_file::MmapFileConfig;
use apollo_storage::{StorageConfig, StorageScope};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

use crate::class_storage::CachedClassStorageConfig;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct ClassHashDbConfig {
    pub path_prefix: PathBuf,
    pub enforce_file_exists: bool,
    pub min_size: isize,
    pub max_size: isize,
    pub growth_step: isize,
}

impl SerializeConfig for ClassHashDbConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "path_prefix",
                &self.path_prefix,
                "Prefix of the path of the node's storage directory.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enforce_file_exists",
                &self.enforce_file_exists,
                "Whether to enforce that the path exists. If true, `open_env` fails when the \
                 mdbx.dat file does not exist.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_size",
                &self.min_size,
                "The minimum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "growth_step",
                &self.growth_step,
                "The growth step in bytes, must be greater than zero to allow the database to \
                 grow.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Validate)]
pub struct ClassHashStorageConfig {
    #[validate]
    pub class_hash_db_config: ClassHashDbConfig,
    #[validate]
    pub mmap_file_config: MmapFileConfig,
    pub scope: StorageScope,
}

impl Default for ClassHashStorageConfig {
    fn default() -> Self {
        Self {
            class_hash_db_config: ClassHashDbConfig {
                path_prefix: "/data/class_hash_storage".into(),
                enforce_file_exists: false,
                min_size: 1 << 20,    // 1MB
                max_size: 1 << 40,    // 1TB
                growth_step: 1 << 32, // 4GB
            },
            mmap_file_config: MmapFileConfig {
                max_size: 1 << 30,        // 1GB.
                growth_step: 1 << 20,     // 1MB.
                max_object_size: 1 << 10, // 1KB; a class hash is 32B.
            },
            scope: StorageScope::StateOnly,
        }
    }
}

impl From<ClassHashStorageConfig> for StorageConfig {
    fn from(value: ClassHashStorageConfig) -> Self {
        Self {
            db_config: apollo_storage::db::DbConfig {
                // TODO(Noamsp): move the chain id into the config and use StorageConfig instead of
                // ClassHashStorageConfig
                chain_id: ChainId::Other("UnusedChainID".to_string()),
                path_prefix: value.class_hash_db_config.path_prefix,
                enforce_file_exists: value.class_hash_db_config.enforce_file_exists,
                min_size: value.class_hash_db_config.min_size,
                max_size: value.class_hash_db_config.max_size,
                growth_step: value.class_hash_db_config.growth_step,
            },
            scope: value.scope,
            mmap_file_config: value.mmap_file_config,
        }
    }
}

impl SerializeConfig for ClassHashStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dumped_config = BTreeMap::from([ser_param(
            "scope",
            &self.scope,
            "The categories of data saved in storage.",
            ParamPrivacyInput::Public,
        )]);
        dumped_config
            .append(&mut prepend_sub_config_name(self.mmap_file_config.dump(), "mmap_file_config"));
        dumped_config.append(&mut prepend_sub_config_name(
            self.class_hash_db_config.dump(),
            "class_hash_db_config",
        ));
        dumped_config
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FsClassStorageConfig {
    pub persistent_root: PathBuf,
    pub class_hash_storage_config: ClassHashStorageConfig,
}

impl Default for FsClassStorageConfig {
    fn default() -> Self {
        Self {
            persistent_root: "/data/classes".into(),
            class_hash_storage_config: Default::default(),
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
        dump
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
    pub max_compiled_contract_class_object_size: usize,
}

impl Default for ClassManagerConfig {
    fn default() -> Self {
        ClassManagerConfig {
            cached_class_storage_config: CachedClassStorageConfig::default(),
            max_compiled_contract_class_object_size: 4089446,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FsClassManagerConfig {
    pub class_manager_config: ClassManagerConfig,
    pub class_storage_config: FsClassStorageConfig,
}

impl SerializeConfig for FsClassManagerConfig {
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
