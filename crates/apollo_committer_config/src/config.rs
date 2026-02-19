use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use starknet_patricia_storage::map_storage::{CachedStorageConfig, PatriciaCachedStorage};
use starknet_patricia_storage::rocksdb_storage::{RocksDbStorage, RocksDbStorageConfig};
use starknet_patricia_storage::storage_trait::{Storage, StorageConfigTrait};
use validator::{Validate, ValidationError, ValidationErrors};

pub type ApolloStorage = PatriciaCachedStorage<RocksDbStorage>;

pub type ApolloCommitterConfig = CommitterConfig<<ApolloStorage as Storage>::Config>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

impl Validate for CommitterConfig<CachedStorageConfig<RocksDbStorageConfig>> {
    fn validate(&self) -> Result<(), ValidationErrors> {
        // Validate nested storage config.
        self.storage_config.validate()?;

        // Cross-field validation: building storage tries concurrently requires spawn_blocking_reads
        // to be enabled, otherwise the concurrent tasks will block the async runtime.
        if self.reader_config.build_storage_tries_concurrently()
            && !self.storage_config.inner_storage_config.spawn_blocking_reads
        {
            let mut errors = ValidationErrors::new();
            errors.add(
                "storage_config",
                ValidationError::new(
                    "spawn_blocking_reads must be true when build_storage_tries_concurrently is \
                     true",
                ),
            );
            return Err(errors);
        }

        Ok(())
    }
}
