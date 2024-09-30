use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The batcher related configuration.
/// TODO(Lev/Tsabary/Yael/Dafna): Define actual configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    pub storage: papyrus_storage::StorageConfig,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        append_sub_config_name(self.storage.dump(), "storage")
    }
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            storage: papyrus_storage::StorageConfig {
                db_config: papyrus_storage::db::DbConfig {
                    path_prefix: ".".into(),
                    // By default we don't want to create the DB if it doesn't exist.
                    enforce_file_exists: true,
                    ..Default::default()
                },
                scope: papyrus_storage::StorageScope::StateOnly,
                ..Default::default()
            },
        }
    }
}
