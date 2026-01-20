use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig};
use starknet_patricia_storage::rocksdb_storage::RocksDbStorage;
use starknet_patricia_storage::storage_trait::{Storage, StorageConfigTrait};
use validator::Validate;

// 1M size cache.
pub const CACHE_MAX_ENTRIES: usize = 1000000;

pub type ApolloStorage = CachedStorage<RocksDbStorage>;

pub type ApolloCommitterConfig = CommitterConfig<<ApolloStorage as Storage>::Config>;

pub const APOLLO_CACHE_STORAGE_CONFIG: CachedStorageConfig = CachedStorageConfig {
    cache_size: NonZeroUsize::new(CACHE_MAX_ENTRIES).unwrap(),
    cache_on_write: true,
    include_inner_stats: true,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig<C: StorageConfigTrait> {
    pub reader_config: ReaderConfig,
    pub db_path: PathBuf,
    pub storage_config: C,
    pub verify_state_diff_hash: bool,
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
        ]);
        dump.extend(prepend_sub_config_name(self.reader_config.dump(), "reader_config"));
        dump.extend(prepend_sub_config_name(self.storage_config.dump(), "storage_config"));
        dump
    }
}

impl<C: StorageConfigTrait> Default for CommitterConfig<C> {
    fn default() -> Self {
        Self {
            reader_config: ReaderConfig::default(),
            db_path: "/data/committer".into(),
            storage_config: C::default(),
            verify_state_diff_hash: true,
        }
    }
}
