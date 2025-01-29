use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::class_storage::{CachedClassStorageConfig, ClassHashStorageConfig};

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
