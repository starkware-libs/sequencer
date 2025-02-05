use std::collections::BTreeMap;
use std::path::PathBuf;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::class_storage::CachedClassStorageConfig;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClassHashStorageConfig {
    pub path_prefix: PathBuf,
    pub enforce_file_exists: bool,
    pub max_size: usize,
}

impl Default for ClassHashStorageConfig {
    fn default() -> Self {
        Self {
            path_prefix: "/data".into(),
            enforce_file_exists: false,
            max_size: 1 << 20, // 1MB.
        }
    }
}

impl SerializeConfig for ClassHashStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "path_prefix",
                &self.path_prefix,
                "Prefix of the path of class hash storage directory.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enforce_file_exists",
                &self.enforce_file_exists,
                "Whether to enforce that the above path exists.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of the class hash storage in bytes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FsClassStorageConfig {
    pub persistent_root: PathBuf,
    pub class_hash_storage_config: ClassHashStorageConfig,
}

impl Default for FsClassStorageConfig {
    fn default() -> Self {
        Self { persistent_root: "/classes".into(), class_hash_storage_config: Default::default() }
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
        dump.append(&mut append_sub_config_name(
            self.class_hash_storage_config.dump(),
            "class_hash_storage_config",
        ));
        dump
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
}

impl SerializeConfig for ClassManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::new();
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
