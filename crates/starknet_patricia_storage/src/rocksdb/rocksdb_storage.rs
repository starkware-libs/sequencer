use std::path::Path;

use itertools::Itertools;
use rust_rocksdb::checkpoint::Checkpoint;
use rust_rocksdb::properties::{ESTIMATE_PENDING_COMPACTION_BYTES, TOTAL_SST_FILES_SIZE};
use rust_rocksdb::statistics::StatsLevel;
use rust_rocksdb::{
    BlockBasedIndexType,
    BlockBasedOptions,
    Cache,
    ColumnFamily,
    ColumnFamilyDescriptor,
    DBCommon,
    Options,
    ReadOptions,
    SliceTransform,
    WriteBatch,
    WriteOptions,
    DB,
    DEFAULT_COLUMN_FAMILY_NAME,
};
use tracing::info;

use super::RocksdbStorageStats;
use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    PatriciaStorageError,
    PatriciaStorageResult,
    Storage,
    TrieKey,
};
// General database Options.

const DB_BLOCK_SIZE: usize = 8 * 1024; // 8KB
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

const NUM_THREADS: i32 = 16;
// Maximum number of background compactions (STT files merge and rewrite) and flushes.
const MAX_BACKGROUND_JOBS: i32 = 8;

// Column familiy descriptors.
pub const LATEST_TRIE_CF: &str = "latest_trie";
pub const HISTORICAL_TRIES_CF: &str = "historical_tries";

const TIMESTAMP_BYTE_SIZE: usize = 8;

const MAX_RECENT_HISTORY_DB_SIZE: u64 = 1024 * 1024 * 1024 * 100; // 100GB
const MAX_COMPACTION_DEBT: u64 = 1024 * 1024 * 1024 * 50; // 50GB

pub struct CfOptions {
    pub options: Options,
    // Used for stats
    pub cache_handle: Cache,
}

pub struct RocksDbOptions {
    pub general_db_options: Options,
    pub latest_cf_options: CfOptions,
    pub historical_cf_options: CfOptions,
    pub write_options: WriteOptions,
}

impl Default for RocksDbOptions {
    fn default() -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        opts.set_bytes_per_sync(BYTES_PER_SYNC);
        opts.set_write_buffer_size(WRITE_BUFFER_SIZE);
        opts.increase_parallelism(NUM_THREADS);
        opts.set_max_background_jobs(MAX_BACKGROUND_JOBS);
        opts.set_max_write_buffer_number(MAX_WRITE_BUFFERS);

        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(KEY_PREFIX_BYTES_LENGTH));

        opts.enable_statistics();
        opts.set_statistics_level(StatsLevel::ExceptTimers);

        let mut latest_cf_options = opts.clone();
        let (latest_block_options, latest_cache_handle) = get_latest_cf_block_options();
        // We must have "latest" as a timestamped DB since after the checkpoint we will use TS on
        // writes.
        latest_cf_options.set_block_based_table_factory(&latest_block_options);
        latest_cf_options.set_comparator_with_ts(
            "bytewise+u64ts",
            TIMESTAMP_BYTE_SIZE,
            Box::new(|a, b| a.cmp(b)),
            Box::new(|tsa, tsb| be_u64(tsa).cmp(&be_u64(tsb))),
            Box::new(|a, a_has_ts, b, b_has_ts| {
                let a_key = if a_has_ts { &a[..a.len() - TIMESTAMP_BYTE_SIZE] } else { a };
                let b_key = if b_has_ts { &b[..b.len() - TIMESTAMP_BYTE_SIZE] } else { b };
                a_key.cmp(b_key)
            }),
        );

        let mut historical_cf_options = opts.clone();
        let (historical_block_options, historical_cache_handle) = get_historical_cf_block_options();
        historical_cf_options.set_block_based_table_factory(&historical_block_options);
        historical_cf_options.set_comparator_with_ts(
            "bytewise+u64ts",
            TIMESTAMP_BYTE_SIZE,
            Box::new(|a, b| a.cmp(b)),
            Box::new(|tsa, tsb| be_u64(tsa).cmp(&be_u64(tsb))),
            Box::new(|a, a_has_ts, b, b_has_ts| {
                let a_key = if a_has_ts { &a[..a.len() - TIMESTAMP_BYTE_SIZE] } else { a };
                let b_key = if b_has_ts { &b[..b.len() - TIMESTAMP_BYTE_SIZE] } else { b };
                a_key.cmp(b_key)
            }),
        );

        // Jimmy historical options
        historical_cf_options.set_write_buffer_size(WRITE_BUFFER_SIZE * 10);
        historical_cf_options.set_use_direct_io_for_flush_and_compaction(true);
        historical_cf_options.set_level_compaction_dynamic_level_bytes(true);
        historical_cf_options.set_target_file_size_base(256 * 1024 * 1024);

        // ~300 MB/s cap for history DB
        let rate_bytes_per_sec = 300 * 1024 * 1024;
        let refill_period_ms = 10;
        let fairness = 10;

        historical_cf_options.set_ratelimiter(rate_bytes_per_sec, refill_period_ms, fairness);

        // Set write options.
        let mut write_options = WriteOptions::default();

        // disable fsync after each write
        write_options.set_sync(false);
        // no write ahead log at all
        write_options.disable_wal(true);

        RocksDbOptions {
            general_db_options: opts,
            latest_cf_options: CfOptions {
                options: latest_cf_options,
                cache_handle: latest_cache_handle,
            },
            historical_cf_options: CfOptions {
                options: historical_cf_options,
                cache_handle: historical_cache_handle,
            },
            write_options,
        }
    }
}

fn get_latest_cf_block_options() -> (BlockBasedOptions, Cache) {
    let mut block = BlockBasedOptions::default();
    let cache = Cache::new_lru_cache(DB_CACHE_SIZE);
    block.set_block_cache(&cache);

    // With a single level, filter blocks become too large to sit in cache.
    block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
    block.set_partition_filters(true);

    block.set_block_size(DB_BLOCK_SIZE);
    block.set_cache_index_and_filter_blocks(true);
    // Make sure filter blocks are cached.
    block.set_pin_l0_filter_and_index_blocks_in_cache(true);

    block.set_bloom_filter(BLOOM_FILTER_NUM_BITS, false);

    (block, cache)
}

fn get_historical_cf_block_options() -> (BlockBasedOptions, Cache) {
    let mut block = BlockBasedOptions::default();
    let cache = Cache::new_lru_cache(DB_CACHE_SIZE / 4);
    block.set_block_cache(&cache);

    // With a single level, filter blocks become too large to sit in cache.
    // block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
    // block.set_partition_filters(true);

    block.set_block_size(DB_BLOCK_SIZE);
    block.set_cache_index_and_filter_blocks(false);
    // Make sure filter blocks are cached.
    block.set_pin_l0_filter_and_index_blocks_in_cache(false);

    (block, cache)
}

impl RocksDbOptions {
    pub fn default_mmap_enabled() -> Self {
        let mut opts = Self::default();
        opts.historical_cf_options.options.set_allow_mmap_reads(true);
        opts.latest_cf_options.options.set_allow_mmap_writes(true);
        opts
    }
}

pub struct RocksDbStorage {
    pub(crate) latest_db: DB,
    pub(crate) recent_history_db: DB,
    pub(crate) old_history_dbs: Vec<DB>,
    pub(crate) write_options: WriteOptions,
    // Following fields are used for stats.
    pub(crate) db_options: Options,
    pub(crate) history_options: Options,
    pub(crate) latest_cf_cache_handle: Cache,
    pub(crate) historical_cf_cache_handle: Cache,
}

fn be_u64(bytes: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes[..8]);
    u64::from_be_bytes(b)
}

impl RocksDbStorage {
    pub fn get_db_options(&self) -> &Options {
        &self.db_options
    }

    pub fn open(path: &Path, options: RocksDbOptions) -> PatriciaStorageResult<Self> {
        let latest_path = path.join("latest");
        let history_path = path.join("history");

        let latest_db = open_db_with_default_cf(&latest_path, &options.latest_cf_options.options)?;
        let history_db =
            open_db_with_default_cf(&history_path, &options.historical_cf_options.options)?;

        Ok(Self {
            latest_db,
            recent_history_db: history_db,
            old_history_dbs: vec![],
            write_options: options.write_options,
            db_options: options.general_db_options,
            history_options: options.historical_cf_options.options,
            latest_cf_cache_handle: options.latest_cf_options.cache_handle,
            historical_cf_cache_handle: options.historical_cf_options.cache_handle,
        })
    }

    pub fn maybe_create_new_checkpoint(&mut self) -> PatriciaStorageResult<()> {
        if self.should_create_new_checkpoint()? {
            info!("RocksDbStorage: Creating new checkpoint {}", self.old_history_dbs.len());

            let new_checkpoint_path = {
                let checkpoint = Checkpoint::new(&self.latest_db)?;
                let previous_checkpoint_path = self.recent_history_db.path();
                let new_checkpoint_path = previous_checkpoint_path
                    .parent()
                    .unwrap()
                    .join(self.old_history_dbs.len().to_string());
                checkpoint.create_checkpoint(&new_checkpoint_path)?;
                new_checkpoint_path
            };

            let new_checkpoint_db =
                open_db_with_default_cf(&new_checkpoint_path, &self.history_options)?;
            let old_db = std::mem::replace(&mut self.recent_history_db, new_checkpoint_db);
            // Consider stop working on the older history DB.
            // old_db.set_options(&[("disable_auto_compactions", "true")])?;

            self.old_history_dbs.push(old_db);

            info!("RocksDbStorage: Created new checkpoint {}", self.old_history_dbs.len());
        }

        Ok(())
    }

    fn should_create_new_checkpoint(&self) -> PatriciaStorageResult<bool> {
        let total_sst_bytes =
            self.recent_history_db.property_int_value(TOTAL_SST_FILES_SIZE.as_str())?.unwrap_or(0);
        let pending_compaction_bytes = self
            .recent_history_db
            .property_int_value(ESTIMATE_PENDING_COMPACTION_BYTES)?
            .unwrap_or(0);

        let should_create = total_sst_bytes > MAX_RECENT_HISTORY_DB_SIZE
            || pending_compaction_bytes > MAX_COMPACTION_DEBT;

        if should_create {
            info!(
                "RocksDbStorage: SST size is {} and compaction debt is {} ",
                total_sst_bytes / (1024 * 1024),
                pending_compaction_bytes / (1024 * 1024)
            );
        };

        Ok(should_create)
    }
}

fn open_db_with_default_cf(path: &Path, options: &Options) -> PatriciaStorageResult<DB> {
    let cf_desc = ColumnFamilyDescriptor::new(DEFAULT_COLUMN_FAMILY_NAME, options.clone());
    Ok(DB::open_cf_descriptors(&options, path, vec![cf_desc])?)
}

trait RocksDbKey<'a> {
    fn get_db(&self, storage: &'a RocksDbStorage) -> &'a DB;
    fn get_timestamp(&self) -> Option<u64>;
}

impl<'a> RocksDbKey<'a> for TrieKey {
    fn get_db(&self, storage: &'a RocksDbStorage) -> &'a DB {
        match self {
            TrieKey::LatestTrie(_) => &storage.latest_db,
            TrieKey::HistoricalTries(_, _) => &storage.recent_history_db,
        }
    }

    fn get_timestamp(&self) -> Option<u64> {
        match self {
            TrieKey::HistoricalTries(_, block_number) => Some(block_number.0),
            TrieKey::LatestTrie(_) => None,
        }
    }
}

impl Storage for RocksDbStorage {
    type Stats = RocksdbStorageStats;

    fn get(&mut self, key: &TrieKey) -> PatriciaStorageResult<Option<DbValue>> {
        let db = key.get_db(self);
        let timestamp = key.get_timestamp();

        let mut read_options = ReadOptions::default();
        if let Some(timestamp) = timestamp {
            read_options.set_timestamp(timestamp.to_be_bytes());
        }

        let raw_key: &DbKey = key.into();
        Ok(db.get_opt(&raw_key.0, &read_options)?.map(DbValue))
    }

    fn set(&mut self, key: TrieKey, value: DbValue) -> PatriciaStorageResult<()> {
        let db = key.get_db(self);
        let timestamp = key.get_timestamp();
        let raw_key: DbKey = key.into();

        if let Some(timestamp) = timestamp {
            Ok(db.put_with_ts_opt(
                &raw_key.0,
                timestamp.to_be_bytes(),
                &value.0,
                &self.write_options,
            )?)
        } else {
            Ok(db.put_opt(&raw_key.0, &value.0, &self.write_options)?)
        }
    }

    fn mget(&mut self, keys: &[&TrieKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        if keys.is_empty() {
            return Ok(vec![]);
        }

        let mut timestamps = keys.iter().map(|k| k.get_timestamp());
        let timestamp =
            timestamps.all_equal_value().map_err(|_| PatriciaStorageError::MultipleTimestamps)?;

        let mut read_options = ReadOptions::default();
        read_options.set_timestamp(0_u64.to_be_bytes());

        let db: &DB = if let Some(timestamp) = timestamp {
            read_options.set_timestamp(timestamp.to_be_bytes());
            &self.recent_history_db
        } else {
            &self.latest_db
        };

        let raw_keys = keys.iter().map(|k| {
            let raw_key: &DbKey = (*k).into();
            &raw_key.0
        });
        let res = db
            .multi_get_opt(raw_keys, &read_options)
            .into_iter()
            .map(|r| r.map(|opt| opt.map(DbValue)))
            .collect::<Result<_, _>>()?;
        Ok(res)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        if key_to_value.is_empty() {
            return Ok(());
        }

        let mut timestamps = key_to_value.keys().map(|k| k.get_timestamp());
        let timestamp =
            timestamps.all_equal_value().map_err(|_| PatriciaStorageError::MultipleTimestamps)?;

        let db: &DB = if timestamp.is_some() { &self.recent_history_db } else { &self.latest_db };
        let timestamp = timestamp.unwrap_or_default();

        let mut batch = WriteBatch::default();

        for key in key_to_value.keys() {
            let raw_key: &DbKey = key.into();
            batch.put_cf_with_ts(
                &db.cf_handle(DEFAULT_COLUMN_FAMILY_NAME).unwrap(),
                &raw_key.0,
                timestamp.to_be_bytes(),
                &key_to_value[key].0,
            );
        }

        Ok(db.write_opt(&batch, &self.write_options)?)
    }

    fn delete(&mut self, key: &TrieKey) -> PatriciaStorageResult<()> {
        let db = key.get_db(self);
        let timestamp = key.get_timestamp();
        if timestamp.is_some() {
            return Err(PatriciaStorageError::AttemptToModifyHistory);
        }

        let raw_key: &DbKey = key.into();
        Ok(db.delete(&raw_key.0)?)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(RocksdbStorageStats::collect(self))
    }

    fn reorder_database(&mut self) -> PatriciaStorageResult<()> {
        self.maybe_create_new_checkpoint()
    }
}
