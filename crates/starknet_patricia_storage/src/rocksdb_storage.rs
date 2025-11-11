use std::path::Path;

use rust_rocksdb::{
    BlockBasedIndexType,
    BlockBasedOptions,
    Cache,
    ColumnFamily,
    ColumnFamilyDescriptor,
    Options,
    WriteBatch,
    WriteOptions,
    DB,
};

use crate::storage_trait::{DbHashMap, DbKey, DbValue, NoStats, PatriciaStorageResult, Storage};

// General database Options.

const DB_BLOCK_SIZE: usize = 4 * 1024; // 4MB
const DB_CACHE_SIZE: usize = 2 * 1024 * 1024 * 1024; // 2GB
// Number of bits in the bloom filter (increase to reduce false positives at the cost of more
// memory).
const BLOOM_FILTER_NUM_BITS: f64 = 10.0;

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
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        opts.set_bytes_per_sync(BYTES_PER_SYNC);
        opts.set_write_buffer_size(WRITE_BUFFER_SIZE);
        opts.increase_parallelism(NUM_THREADS);
        opts.set_max_background_jobs(MAX_BACKGROUND_JOBS);
        opts.set_max_write_buffer_number(MAX_WRITE_BUFFERS);

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

        opts.set_block_based_table_factory(&block);

        // Set write options.
        let mut write_options = WriteOptions::default();

        // disable fsync after each write
        write_options.set_sync(false);
        // no write ahead log at all
        write_options.disable_wal(true);

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

pub struct RocksDbStorage {
    db: DB,
    write_options: WriteOptions,
    /// If true, the database is opened with 256 column families, and each key mapped to a column
    /// family according to its last byte. Otherwise, the database is opened with a single
    /// column family (default behavior).
    column_families: bool,
}

impl RocksDbStorage {
    pub fn open(
        path: &Path,
        options: RocksDbOptions,
        column_families: bool,
    ) -> PatriciaStorageResult<Self> {
        // Default column is required for the database to be opened.
        let mut cfs = vec![ColumnFamilyDescriptor::new("default", options.db_options.clone())];

        if column_families {
            for i in 0..1 << u8::BITS {
                cfs.push(ColumnFamilyDescriptor::new(format!("{i}"), options.db_options.clone()));
            }
        }

        let db = DB::open_cf_descriptors(&options.db_options, path, cfs)?;
        Ok(Self { db, write_options: options.write_options, column_families })
    }

    pub fn get_column_family(&self, key: &DbKey) -> &ColumnFamily {
        if self.column_families {
            let last_byte = key.0.last().unwrap_or(&0u8);
            self.db
                .cf_handle(&format!("{last_byte}"))
                .unwrap_or_else(|| panic!("Column family not found: {last_byte}"))
        } else {
            self.db.cf_handle("default").unwrap()
        }
    }
}

impl Storage for RocksDbStorage {
    type Stats = NoStats;

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let cf = self.get_column_family(key);
        Ok(self.db.get_cf(&cf, &key.0)?.map(DbValue))
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        let cf = self.get_column_family(&key);
        Ok(self.db.put_cf_opt(&cf, &key.0, &value.0, &self.write_options)?)
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let cfs_and_raw_keys = keys.iter().map(|k| (self.get_column_family(k), k.0.clone()));
        let res = self
            .db
            .multi_get_cf(cfs_and_raw_keys)
            .into_iter()
            .map(|r| r.map(|opt| opt.map(DbValue)))
            .collect::<Result<_, _>>()?;
        Ok(res)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let mut batch = WriteBatch::default();
        for key in key_to_value.keys() {
            let cf = self.get_column_family(key);
            batch.put_cf(&cf, &key.0, &key_to_value[key].0);
        }
        Ok(self.db.write_opt(&batch, &self.write_options)?)
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        let cf = self.get_column_family(key);
        Ok(self.db.delete_cf_opt(&cf, &key.0, &self.write_options)?)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}
