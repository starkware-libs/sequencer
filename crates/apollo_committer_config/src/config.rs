use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use validator::Validate;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig {
    pub reader_config: ReaderConfig,
    pub storage_config: CommitterStorageConfig,
    pub verify_state_diff_hash: bool,
}

impl SerializeConfig for CommitterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([ser_param(
            "verify_state_diff_hash",
            &self.verify_state_diff_hash,
            "If true, the committer will verify the state diff hash.",
            ParamPrivacyInput::Public,
        )]);
        dump.extend(prepend_sub_config_name(self.reader_config.dump(), "reader_config"));
        dump.extend(prepend_sub_config_name(self.storage_config.dump(), "storage_config"));
        dump
    }
}

impl Default for CommitterConfig {
    fn default() -> Self {
        Self {
            reader_config: ReaderConfig::default(),
            storage_config: CommitterStorageConfig::default(),
            verify_state_diff_hash: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterStorageConfig {
    pub path: PathBuf,
}

impl Default for CommitterStorageConfig {
    fn default() -> Self {
        Self { path: "/data/committer".into() }
    }
}

impl SerializeConfig for CommitterStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "path",
            &self.path,
            "Path to the committer storage directory.",
            ParamPrivacyInput::Public,
        )])
    }
}
