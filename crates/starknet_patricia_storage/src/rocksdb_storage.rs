use core::fmt;
use std::path::Path;
use std::sync::Arc;

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

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbValue,
    EmptyStorageConfig,
    PatriciaStorageResult,
    Storage,
    StorageStats,
};

// General database Options.

const DB_CACHE_SIZE: usize = 8 * 1024 * 1024 * 1024; // 8GB
// Number of bits in the bloom filter (increase to reduce false positives at the cost of more
// memory).
const BLOOM_FILTER_NUM_BITS: f64 = 10.0;
const KEY_PREFIX_BYTES_LENGTH: usize = 32;

// Write Options.

// Allows OS to incrementally sync files to disk as they are written.
const BYTES_PER_SYNC: u64 = 1024 * 1024; // 1MB
// Amount of data to build up in memory before writing to disk.
const WRITE_BUFFER_SIZE: usize = 128 * 1024 * 1024; // 128MB
const MAX_WRITE_BUFFERS: i32 = 4;

// Concurrency Options.
// TODO(Nimrod): Make this configurable based on the machine's CPU cores.
const NUM_THREADS: i32 = 8;
// Maximum number of background compactions (STT files merge and rewrite) and flushes.
const MAX_BACKGROUND_JOBS: i32 = 8;

pub struct RocksDbOptions {
    pub db_options: Options,
    pub write_options: WriteOptions,
}

impl Default for RocksDbOptions {
    fn default() -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        opts.set_bytes_per_sync(BYTES_PER_SYNC);
        opts.set_write_buffer_size(WRITE_BUFFER_SIZE);
        opts.increase_parallelism(NUM_THREADS);
        opts.set_max_subcompactions(NUM_THREADS.try_into().unwrap());
        opts.set_max_background_jobs(MAX_BACKGROUND_JOBS);
        opts.set_max_write_buffer_number(MAX_WRITE_BUFFERS);
        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(KEY_PREFIX_BYTES_LENGTH));

        let mut block = BlockBasedOptions::default();
        let cache = Cache::new_lru_cache(DB_CACHE_SIZE);
        block.set_block_cache(&cache);

        // With a single level, filter blocks become too large to sit in cache.
        block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
        block.set_partition_filters(true);

        block.set_cache_index_and_filter_blocks(true);
        // Make sure filter blocks are cached.
        block.set_pin_l0_filter_and_index_blocks_in_cache(true);

        block.set_bloom_filter(BLOOM_FILTER_NUM_BITS, false);

        // Enable statistics collection.
        opts.enable_statistics();
        opts.set_statistics_level(StatsLevel::ExceptDetailedTimers);

        opts.set_block_based_table_factory(&block);

        let mut write_options = WriteOptions::default();
        write_options.set_sync(true);

        RocksDbOptions { db_options: opts, write_options }
    }
}

impl RocksDbOptions {
    pub fn default_no_mmap() -> Self {
        let mut opts = Self::default();
        opts.db_options.set_allow_mmap_reads(false);
        opts.db_options.set_allow_mmap_writes(false);
        opts
    }
}

#[derive(Clone)]
pub struct RocksDbStorage {
    db: Arc<DB>,
    options: Arc<RocksDbOptions>,
}

impl RocksDbStorage {
    pub fn open(path: &Path, options: RocksDbOptions) -> PatriciaStorageResult<Self> {
        let db = Arc::new(DB::open(&options.db_options, path)?);
        let options = Arc::new(options);
        Ok(Self { db, options })
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
    type Config = EmptyStorageConfig;

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

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(RocksDbStats(
            self.options
                .db_options
                .get_statistics()
                .expect("Statistics are unexpectedly disabled."),
        ))
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        Some(self.clone())
    }
}
