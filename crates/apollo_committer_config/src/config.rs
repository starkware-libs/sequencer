use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use starknet_patricia_storage::map_storage::CachedStorage;
use starknet_patricia_storage::rocksdb_storage::RocksDbStorage;
use starknet_patricia_storage::storage_trait::{Storage, StorageConfigTrait};
use validator::Validate;

pub type ApolloStorage = CachedStorage<RocksDbStorage>;

pub type ApolloCommitterConfig = CommitterConfig<<ApolloStorage as Storage>::Config>;

pub const DEFAULT_COMMIT_DURATION_WARN_THRESHOLD: Duration = Duration::from_millis(3000);
pub const DEFAULT_COMMITMENT_INFOS_RETENTION_BLOCKS: u64 = 1000;
pub const DEFAULT_MAX_COMMITMENT_INFOS_DELETIONS_PER_COMMIT: u64 = 100;

/// Configuration of commitment-infos pruning from the committer storage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitmentInfosPruningConfig {
    /// The commitment infos of the `retention_blocks` highest committed heights are kept in
    /// storage; those of lower heights are pruned when committing a block.
    #[validate(range(min = 10))]
    pub retention_blocks: u64,
    /// At most this many heights are pruned per commit.
    pub max_deletions_per_commit: u64,
}

impl SerializeConfig for CommitmentInfosPruningConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "retention_blocks",
                &self.retention_blocks,
                "The commitment infos of this many highest committed heights are kept in storage; \
                 those of lower heights are pruned when committing a block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_deletions_per_commit",
                &self.max_deletions_per_commit,
                "At most this many heights' commitment infos are pruned per commit.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for CommitmentInfosPruningConfig {
    fn default() -> Self {
        Self {
            retention_blocks: DEFAULT_COMMITMENT_INFOS_RETENTION_BLOCKS,
            max_deletions_per_commit: DEFAULT_MAX_COMMITMENT_INFOS_DELETIONS_PER_COMMIT,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig<C: StorageConfigTrait> {
    pub reader_config: ReaderConfig,
    pub db_path: PathBuf,
    pub storage_config: C,
    pub verify_state_diff_hash: bool,
    /// If true, `read_paths_and_commit_block` requests are served as `commit_block` requests,
    /// treating the accessed keys as an empty set.
    pub serve_read_paths_as_commit_block: bool,
    /// Commit durations above this threshold (in milliseconds) are logged at WARN level.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub commit_duration_warn_threshold_millis: Duration,
    /// If None, no commitment infos are pruned.
    #[validate(nested)]
    pub commitment_infos_pruning_config: Option<CommitmentInfosPruningConfig>,
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
                "serve_read_paths_as_commit_block",
                &self.serve_read_paths_as_commit_block,
                "If true, read_paths_and_commit_block requests are served as commit_block \
                 requests, treating the accessed keys as an empty set.",
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
        dump.extend(ser_optional_sub_config(
            &self.commitment_infos_pruning_config,
            "commitment_infos_pruning_config",
        ));
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
            serve_read_paths_as_commit_block: false,
            commit_duration_warn_threshold_millis: DEFAULT_COMMIT_DURATION_WARN_THRESHOLD,
            commitment_infos_pruning_config: None,
        }
    }
}
