use core::fmt;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use rust_rocksdb::statistics::StatsLevel;
use rust_rocksdb::{
    BlockBasedIndexType,
    BlockBasedOptions,
    Cache,
    Options,
    SliceTransform,
    WriteBatch,
    WriteOptions,
    DB,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbOperation,
    DbOperationMap,
    DbValue,
    PatriciaStorageError,
    PatriciaStorageResult,
    Storage,
    StorageConfigTrait,
    StorageStats,
};

// General database Options.

const DB_CACHE_SIZE: usize = 8 * 1024 * 1024 * 1024; // 8GB
// Number of bits in the bloom filter (increase to reduce false positives at the cost of more
// memory).
const BLOOM_FILTER_NUM_BITS: u32 = 10;
const KEY_PREFIX_BYTES_LENGTH: usize = 32;

// Write Options.

// Allows OS to incrementally sync files to disk as they are written.
const BYTES_PER_SYNC: u64 = 1024 * 1024; // 1MB
// Amount of data to build up in memory before writing to disk.
const WRITE_BUFFER_SIZE: usize = 128 * 1024 * 1024; // 128MB
const MAX_WRITE_BUFFERS: i32 = 4;

// Concurrency Options.

const NUM_THREADS: i32 = 8;
// Maximum number of background compactions (STT files merge and rewrite) and flushes.
const MAX_BACKGROUND_JOBS: i32 = 8;

pub struct RocksDbOptions {
    pub db_options: Options,
    pub write_options: WriteOptions,
}

impl Default for RocksDbOptions {
    fn default() -> Self {
        Self::from_config(&RocksDbStorageConfig::default())
    }
}

impl RocksDbOptions {
    pub fn from_config(config: &RocksDbStorageConfig) -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        opts.set_bytes_per_sync(config.bytes_per_sync);
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.increase_parallelism(
            config.num_threads.try_into().expect("num_threads should fit in i32"),
        );
        opts.set_max_subcompactions(config.max_subcompactions);
        opts.set_max_background_jobs(
            config.max_background_jobs.try_into().expect("max_background_jobs should fit in i32"),
        );
        opts.set_max_write_buffer_number(
            config.max_write_buffers.try_into().expect("max_write_buffers should fit in i32"),
        );
        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(KEY_PREFIX_BYTES_LENGTH));

        opts.set_allow_mmap_reads(config.use_mmap_reads);

        let mut block = BlockBasedOptions::default();
        let cache = Cache::new_lru_cache(config.cache_size);
        block.set_block_cache(&cache);

        // With a single level, filter blocks become too large to sit in cache.
        block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
        block.set_partition_filters(true);

        block.set_cache_index_and_filter_blocks(true);
        // Make sure filter blocks are cached.
        block.set_pin_l0_filter_and_index_blocks_in_cache(true);

        block.set_bloom_filter(config.bloom_filter_bits.into(), false);

        // Statistics options.
        if config.enable_statistics {
            opts.enable_statistics();
            opts.set_statistics_level(StatsLevel::ExceptDetailedTimers);
        }

        opts.set_block_based_table_factory(&block);

        let mut write_options = WriteOptions::default();
        write_options.set_sync(true);

        RocksDbOptions { db_options: opts, write_options }
    }
}

#[derive(Clone)]
pub struct RocksDbStorage {
    db: Arc<DB>,
    options: Arc<RocksDbOptions>,
}

/// Configuration for RocksDB storage.
///
/// This config is serializable and can be used with the apollo config system.
/// The RocksDB options are not directly serializable, so we store the tunable
/// parameters here and reconstruct `RocksDbOptions` when creating the storage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RocksDbStorageConfig {
    /// Number of threads (used for `increase_parallelism`).
    pub num_threads: usize,
    /// Maximum number of background compaction and flush jobs.
    pub max_background_jobs: usize,
    /// Bytes to sync to disk incrementally during writes.
    pub bytes_per_sync: u64,
    /// Amount of data to build up in memory before writing to disk.
    pub write_buffer_size: usize,
    /// Maximum number of write buffers (memtables).
    pub max_write_buffers: usize,
    /// Maximum number of subcompactions for parallel compaction.
    pub max_subcompactions: u32,
    /// Size of the block cache in bytes.
    pub cache_size: usize,
    /// Number of bits in the bloom filter per key.
    pub bloom_filter_bits: u32,
    /// Flag that determines whether to enable RocksDB statistics collection
    pub enable_statistics: bool,
    /// Whether to use mmap for reading SST files.
    pub use_mmap_reads: bool,
}

impl Default for RocksDbStorageConfig {
    fn default() -> Self {
        Self {
            num_threads: NUM_THREADS.try_into().expect("NUM_THREADS should fit in usize"),
            max_background_jobs: MAX_BACKGROUND_JOBS
                .try_into()
                .expect("MAX_BACKGROUND_JOBS should fit in usize"),
            bytes_per_sync: BYTES_PER_SYNC,
            write_buffer_size: WRITE_BUFFER_SIZE,
            max_write_buffers: MAX_WRITE_BUFFERS
                .try_into()
                .expect("MAX_WRITE_BUFFERS should fit in usize"),
            max_subcompactions: NUM_THREADS.try_into().expect("NUM_THREADS should fit in u32"),
            cache_size: DB_CACHE_SIZE,
            bloom_filter_bits: BLOOM_FILTER_NUM_BITS,
            enable_statistics: true,
            use_mmap_reads: false,
        }
    }
}

impl Validate for RocksDbStorageConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

impl SerializeConfig for RocksDbStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "num_threads",
                &self.num_threads,
                "Number of threads for parallelism",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_background_jobs",
                &self.max_background_jobs,
                "Maximum number of background compaction and flush jobs",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "bytes_per_sync",
                &self.bytes_per_sync,
                "Bytes to sync to disk incrementally during writes",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "write_buffer_size",
                &self.write_buffer_size,
                "Amount of data to build up in memory before writing to disk",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_write_buffers",
                &self.max_write_buffers,
                "Maximum number of write buffers (memtables)",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_subcompactions",
                &self.max_subcompactions,
                "Maximum number of subcompactions for parallel compaction",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "cache_size",
                &self.cache_size,
                "Size of the block cache in bytes",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "bloom_filter_bits",
                &self.bloom_filter_bits,
                "Number of bits in the bloom filter per key",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enable_statistics",
                &self.enable_statistics,
                "Flag that determines whether to enable RocksDB statistics collection",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "use_mmap_reads",
                &self.use_mmap_reads,
                "Whether to use mmap for reading SST files",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl StorageConfigTrait for RocksDbStorageConfig {}

impl RocksDbStorage {
    pub fn new(path: &Path, config: RocksDbStorageConfig) -> PatriciaStorageResult<Self> {
        let options = RocksDbOptions::from_config(&config);
        let db = Arc::new(DB::open(&options.db_options, path)?);
        Ok(Self { db, options: Arc::new(options) })
    }
}

#[derive(Debug, Default)]
pub struct RocksDbStats(String);

impl fmt::Display for RocksDbStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RocksDbStats({})", self.0)
    }
}

impl StorageStats for RocksDbStats {
    // TODO(Nimrod): See if you can extract actual columns and not all in one column.
    fn column_titles() -> Vec<&'static str> {
        vec!["rocksdb_stats"]
    }

    fn column_values(&self) -> Vec<String> {
        vec![self.0.clone()]
    }
}

impl Storage for RocksDbStorage {
    type Stats = RocksDbStats;
    type Config = RocksDbStorageConfig;

    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.db.get(&key.0)?.map(DbValue))
    }

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        Ok(self.db.put_opt(&key.0, &value.0, &self.options.write_options)?)
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let raw_keys = keys.iter().map(|k| &k.0);
        let res = self
            .db
            .multi_get(raw_keys)
            .into_iter()
            .map(|r| r.map(|opt| opt.map(DbValue)))
            .collect::<Result<_, _>>()?;
        Ok(res)
    }

    async fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let mut batch = WriteBatch::default();
        for key in key_to_value.keys() {
            batch.put(&key.0, &key_to_value[key].0);
        }
        Ok(self.db.write_opt(&batch, &self.options.write_options)?)
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        Ok(self.db.delete_opt(&key.0, &self.options.write_options)?)
    }

    async fn multi_set_and_delete(
        &mut self,
        key_to_operation: DbOperationMap,
    ) -> PatriciaStorageResult<()> {
        let mut batch = WriteBatch::default();
        for (key, operation) in key_to_operation.iter() {
            match operation {
                DbOperation::Set(value) => batch.put(&key.0, &value.0),
                DbOperation::Delete => batch.delete(&key.0),
            }
        }
        Ok(self.db.write_opt(&batch, &self.options.write_options)?)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(RocksDbStats(
            self.options.db_options.get_statistics().ok_or(PatriciaStorageError::NoStats)?,
        ))
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        Some(self.clone())
    }
}
