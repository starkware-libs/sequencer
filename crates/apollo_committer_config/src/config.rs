use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use starknet_committer::db::index_db::db::N_COMMITMENT_INFOS_KEYS_PER_BLOCK;
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig};
use starknet_patricia_storage::rocksdb_storage::{RocksDbStorage, RocksDbStorageConfig};
use starknet_patricia_storage::storage_trait::{EmptyStorageConfig, StorageConfigTrait};
use validator::Validate;

pub type ApolloStorage = CachedStorage<RocksDbStorage>;

pub type ApolloStorageConfig = CachedStorageConfig<RocksDbStorageConfig>;

pub type ApolloCommitterConfig = CommitterConfig<ApolloStorageConfig>;

pub const DEFAULT_COMMIT_DURATION_WARN_THRESHOLD: Duration = Duration::from_millis(3000);
/// The commitment infos of this many latest blocks are kept in the storage's short-lived cache.
pub const DEFAULT_SHORT_LIVED_CACHE_BLOCKS: usize = 11;
pub const DEFAULT_SHORT_LIVED_CACHE_SIZE: usize =
    DEFAULT_SHORT_LIVED_CACHE_BLOCKS * N_COMMITMENT_INFOS_KEYS_PER_BLOCK;
pub const DEFAULT_COMMITMENT_INFOS_RETENTION_BLOCKS: u64 = 1000;
pub const DEFAULT_COMMITMENT_INFOS_RANGE_DELETION_INTERVAL_BLOCKS: u64 = 100;

/// Configuration of commitment-infos pruning from the committer storage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitmentInfosPruningConfig {
    /// The commitment infos of the `retention_blocks` highest committed heights are kept in
    /// storage; those of lower heights are deleted by a periodic key range deletion.
    #[validate(range(min = 10))]
    pub retention_blocks: u64,
    /// The commitment infos below the retention window are deleted as one key range, each time
    /// the window advances this many heights. A range deletion lets the storage free their
    /// space in bulk.
    #[validate(range(min = 1))]
    pub range_deletion_interval_blocks: u64,
}

impl Default for CommitmentInfosPruningConfig {
    fn default() -> Self {
        Self {
            retention_blocks: DEFAULT_COMMITMENT_INFOS_RETENTION_BLOCKS,
            range_deletion_interval_blocks: DEFAULT_COMMITMENT_INFOS_RANGE_DELETION_INTERVAL_BLOCKS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig<C: StorageConfigTrait> {
    pub reader_config: ReaderConfig,
    pub db_path: PathBuf,
    pub storage_config: C,
    pub verify_state_diff_hash: bool,
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

impl<C: StorageConfigTrait> CommitterConfig<C> {
    fn default_with_storage_config(storage_config: C) -> Self {
        // TODO(Nimrod): Consider adding dynamic config and move `build_storage_tries_concurrently`
        // to it.
        Self {
            reader_config: ReaderConfig::default(),
            db_path: "/data/committer".into(),
            storage_config,
            verify_state_diff_hash: true,
            commit_duration_warn_threshold_millis: DEFAULT_COMMIT_DURATION_WARN_THRESHOLD,
            commitment_infos_pruning_config: None,
        }
    }
}

impl Default for ApolloCommitterConfig {
    fn default() -> Self {
        Self::default_with_storage_config(ApolloStorageConfig {
            short_lived_cache_size: Some(
                NonZeroUsize::new(DEFAULT_SHORT_LIVED_CACHE_SIZE).unwrap(),
            ),
            ..Default::default()
        })
    }
}

/// The config of a committer over a map storage, which has no cache.
impl Default for CommitterConfig<EmptyStorageConfig> {
    fn default() -> Self {
        Self::default_with_storage_config(EmptyStorageConfig::default())
    }
}
