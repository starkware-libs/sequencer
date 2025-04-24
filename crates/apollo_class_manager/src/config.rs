use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

use crate::class_storage::CachedClassStorageConfig;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FsClassStorageConfig {
    pub persistent_root: PathBuf,
    pub storage_config: StorageConfig,
}

impl Default for FsClassStorageConfig {
    fn default() -> Self {
        Self {
            persistent_root: "/data/classes".into(),
            storage_config: StorageConfig {
                db_config: DbConfig {
                    path_prefix: PathBuf::from("/data/class_hash_storage"),
                    chain_id: ChainId::Other("UnusedChainID".to_string()),
                    ..Default::default()
                },
                scope: apollo_storage::StorageScope::StateOnly,
                mmap_file_config: apollo_storage::mmap_file::MmapFileConfig {
                    max_size: 1 << 30,        // 1GB.
                    growth_step: 1 << 20,     // 1MB.
                    max_object_size: 1 << 10, // 1KB; a class hash is 32B.
                },
            },
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
        dump.append(&mut append_sub_config_name(self.storage_config.dump(), "storage_config"));
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
        dump.append(&mut append_sub_config_name(
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
        dump.append(&mut append_sub_config_name(
            self.class_manager_config.dump(),
            "class_manager_config",
        ));
        dump.append(&mut append_sub_config_name(
            self.class_storage_config.dump(),
            "class_storage_config",
        ));
        dump
    }
}
