use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use starknet_patricia_storage::map_storage::CachedStorage;
use starknet_patricia_storage::rocksdb_storage::RocksDbStorage;
use starknet_patricia_storage::storage_trait::{Storage, StorageConfigTrait};
use validator::Validate;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

pub type ApolloStorage = CachedStorage<RocksDbStorage>;

pub type ApolloCommitterConfig = CommitterConfig<<ApolloStorage as Storage>::Config>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig<C: StorageConfigTrait> {
    pub reader_config: ReaderConfig,
    pub db_path: PathBuf,
    pub storage_config: C,
    pub verify_state_diff_hash: bool,
    /// Commit durations above this threshold (in milliseconds) are logged at WARN level.
    // Deployed nodes ignore schema defaults, so this serde default keeps old configs loadable.
    #[serde(
        default = "default_commit_duration_warn_threshold",
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub commit_duration_warn_threshold_millis: Duration,
}

pub const DEFAULT_COMMIT_DURATION_WARN_THRESHOLD: Duration = Duration::from_millis(3000);

fn default_commit_duration_warn_threshold() -> Duration {
    DEFAULT_COMMIT_DURATION_WARN_THRESHOLD
}

impl<C: StorageConfigTrait> SerializeConfig for CommitterConfig<C> {
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
            ser_param(
                "commit_duration_warn_threshold_millis",
                &self.commit_duration_warn_threshold_millis.as_millis(),
                "Blocks whose commit duration exceeds this threshold (in milliseconds) are logged \
                 at WARN level.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend(prepend_sub_config_name(self.reader_config.dump(), "reader_config"));
        dump.extend(prepend_sub_config_name(self.storage_config.dump(), "storage_config"));
        dump
    }
}

impl<C: StorageConfigTrait> Default for CommitterConfig<C> {
    fn default() -> Self {
        // TODO(Nimrod): Consider adding dynamic config and move `build_storage_tries_concurrently`
        // to it.
        Self {
            reader_config: ReaderConfig::default(),
            db_path: "/data/committer".into(),
            storage_config: C::default(),
            verify_state_diff_hash: true,
            commit_duration_warn_threshold_millis: DEFAULT_COMMIT_DURATION_WARN_THRESHOLD,
        }
    }
}
