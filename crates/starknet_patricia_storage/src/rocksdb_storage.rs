use std::path::Path;

use itertools::Itertools;
use rust_rocksdb::{
    BlockBasedIndexType,
    BlockBasedOptions,
    Cache,
    ColumnFamily,
    ColumnFamilyDescriptor,
    Options,
    ReadOptions,
    SliceTransform,
    WriteBatch,
    WriteOptions,
    DB,
};

use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    NoStats,
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

const NUM_THREADS: i32 = 8;
// Maximum number of background compactions (STT files merge and rewrite) and flushes.
const MAX_BACKGROUND_JOBS: i32 = 8;

// Column familiy descriptors.
const LATEST_TRIE_CF: &str = "latest_trie";
const HISTORICAL_TRIES_CF: &str = "historical_tries";
const TIMESTAMP_BYTE_SIZE: usize = 8;

pub struct RocksDbOptions {
    pub general_db_options: Options,
    pub latest_cf_options: Options,
    pub historical_cf_options: Options,
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

        let mut latest_cf_options = opts.clone();
        latest_cf_options.set_block_based_table_factory(&get_latest_cf_block_options());
        let mut historical_cf_options = opts.clone();
        historical_cf_options.set_block_based_table_factory(&get_historical_cf_block_options());

        // Set write options.
        let mut write_options = WriteOptions::default();

        // disable fsync after each write
        write_options.set_sync(false);
        // no write ahead log at all
        write_options.disable_wal(true);

        RocksDbOptions {
            general_db_options: opts,
            latest_cf_options,
            historical_cf_options,
            write_options,
        }
    }
}

fn get_latest_cf_block_options() -> BlockBasedOptions {
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

    block
}

fn get_historical_cf_block_options() -> BlockBasedOptions {
    let mut block = BlockBasedOptions::default();
    let cache = Cache::new_lru_cache(DB_CACHE_SIZE / 4);
    block.set_block_cache(&cache);

    // With a single level, filter blocks become too large to sit in cache.
    block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
    block.set_partition_filters(true);

    block.set_block_size(DB_BLOCK_SIZE);
    block.set_cache_index_and_filter_blocks(false);
    // Make sure filter blocks are cached.
    block.set_pin_l0_filter_and_index_blocks_in_cache(false);

    block
}

impl RocksDbOptions {
    pub fn default_mmap_enabled() -> Self {
        let mut opts = Self::default();
        opts.historical_cf_options.set_allow_mmap_reads(true);
        opts.latest_cf_options.set_allow_mmap_writes(true);
        opts
    }
}

pub struct RocksDbStorage {
    db: DB,
    write_options: WriteOptions,
}

fn be_u64(bytes: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes[..8]);
    u64::from_be_bytes(b)
}

impl RocksDbStorage {
    pub fn open(path: &Path, options: RocksDbOptions) -> PatriciaStorageResult<Self> {
        let mut hist_cf_opts = options.historical_cf_options;
        hist_cf_opts.set_comparator_with_ts(
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
        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new(LATEST_TRIE_CF, options.latest_cf_options),
            ColumnFamilyDescriptor::new(HISTORICAL_TRIES_CF, hist_cf_opts),
        ];

        let db = DB::open_cf_descriptors(&options.general_db_options, path, cf_descriptors)?;

        Ok(Self { db, write_options: options.write_options })
    }
}

trait RocksDbKey<'a> {
    fn get_cf_handle(&self, storage: &'a RocksDbStorage) -> &'a ColumnFamily;
    fn get_timestamp(&self) -> Option<u64>;
}

impl<'a> RocksDbKey<'a> for TrieKey {
    fn get_cf_handle(&self, storage: &'a RocksDbStorage) -> &'a ColumnFamily {
        match self {
            TrieKey::LatestTrie(_) => storage.db.cf_handle(LATEST_TRIE_CF).unwrap(),
            TrieKey::HistoricalTries(_, _) => storage.db.cf_handle(HISTORICAL_TRIES_CF).unwrap(),
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
    type Stats = NoStats;

    fn get(&mut self, key: &TrieKey) -> PatriciaStorageResult<Option<DbValue>> {
        let cf_handle = key.get_cf_handle(self);
        let timestamp = key.get_timestamp();

        let mut read_options = ReadOptions::default();
        if let Some(timestamp) = timestamp {
            read_options.set_timestamp(timestamp.to_be_bytes());
        }

        let raw_key: &DbKey = key.into();
        Ok(self.db.get_cf_opt(&cf_handle, &raw_key.0, &read_options)?.map(DbValue))
    }

    fn set(&mut self, key: TrieKey, value: DbValue) -> PatriciaStorageResult<()> {
        let cf_handle = key.get_cf_handle(self);
        let timestamp = key.get_timestamp();
        let raw_key: DbKey = key.into();

        if let Some(timestamp) = timestamp {
            Ok(self.db.put_cf_with_ts_opt(
                &cf_handle,
                &raw_key.0,
                timestamp.to_be_bytes(),
                &value.0,
                &self.write_options,
            )?)
        } else {
            Ok(self.db.put_cf_opt(&cf_handle, &raw_key.0, &value.0, &self.write_options)?)
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
        let cf_handle: &ColumnFamily = if let Some(timestamp) = timestamp {
            read_options.set_timestamp(timestamp.to_be_bytes());
            self.db.cf_handle(HISTORICAL_TRIES_CF).unwrap()
        } else {
            self.db.cf_handle(LATEST_TRIE_CF).unwrap()
        };

        let raw_keys = keys.iter().map(|k| {
            let raw_key: &DbKey = (*k).into();
            (cf_handle, &raw_key.0)
        });
        let res = self
            .db
            .multi_get_cf_opt(raw_keys, &read_options)
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

        let mut batch = WriteBatch::default();
        if let Some(timestamp) = timestamp {
            let cf_handle = self.db.cf_handle(HISTORICAL_TRIES_CF).unwrap();
            for key in key_to_value.keys() {
                let raw_key: &DbKey = key.into();
                batch.put_cf_with_ts(
                    &cf_handle,
                    &raw_key.0,
                    timestamp.to_be_bytes(),
                    &key_to_value[key].0,
                );
            }
        } else {
            let cf_handle = self.db.cf_handle(LATEST_TRIE_CF).unwrap();
            for key in key_to_value.keys() {
                let raw_key: &DbKey = key.into();
                batch.put_cf(&cf_handle, &raw_key.0, &key_to_value[key].0);
            }
        }

        Ok(self.db.write_opt(&batch, &self.write_options)?)
    }

    fn delete(&mut self, key: &TrieKey) -> PatriciaStorageResult<()> {
        let cf_handle = key.get_cf_handle(self);
        let timestamp = key.get_timestamp();
        if timestamp.is_some() {
            return Err(PatriciaStorageError::AttemptToModifyHistory);
        }

        let raw_key: &DbKey = key.into();
        Ok(self.db.delete_cf(&cf_handle, &raw_key.0)?)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}
