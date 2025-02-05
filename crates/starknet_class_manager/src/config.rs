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

// TODO(Elin): set appropriated default values.
impl Default for ClassHashStorageConfig {
    fn default() -> Self {
        Self {
            path_prefix: "/data".into(),
            enforce_file_exists: false,
            max_size: 1 << 30, // 1GB.
        }
    }
}

impl SerializeConfig for ClassHashStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "path_prefix",
                &self.path_prefix,
                "Prefix of the path of the node's storage directory",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enforce_file_exists",
                &self.enforce_file_exists,
                "Whether to enforce that the path exists.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FsClassStorageConfig;

impl Default for FsClassStorageConfig {
    fn default() -> Self {
        Self
    }
}

impl SerializeConfig for FsClassStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
    pub storage: ClassHashStorageConfig,
}

impl SerializeConfig for ClassManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::new();
        dump.append(&mut append_sub_config_name(
            self.cached_class_storage_config.dump(),
            "cached_class_storage_config",
        ));
        dump.append(&mut append_sub_config_name(self.storage.dump(), "storage"));
        dump
    }
}
