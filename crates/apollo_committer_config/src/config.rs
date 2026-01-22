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
    pub db_path: PathBuf,
    pub verify_state_diff_hash: bool,
}

impl SerializeConfig for CommitterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "verify_state_diff_hash",
                &self.verify_state_diff_hash,
                "If true, the committer will verify the state diff hash.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "db_path",
                &self.db_path,
                "Path to the committer storage directory.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend(prepend_sub_config_name(self.reader_config.dump(), "reader_config"));
        dump
    }
}

impl Default for CommitterConfig {
    fn default() -> Self {
        Self {
            reader_config: ReaderConfig::default(),
            db_path: "/data/committer".into(),
            verify_state_diff_hash: true,
        }
    }
}
