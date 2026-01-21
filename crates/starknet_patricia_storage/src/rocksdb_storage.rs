use std::path::Path;
use std::sync::Arc;

use rust_rocksdb::{
    BlockBasedIndexType,
    BlockBasedOptions,
    Cache,
    Options,
    WriteBatch,
    WriteOptions,
    DB,
};

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbValue,
    NoStats,
    PatriciaStorageResult,
    Storage,
};

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
        // NOTE: disable_wal(true) means writes may not be persisted if the process is killed.
        // This can cause data loss after a forceful shutdown. Consider enabling WAL for
        // better durability, especially in integration tests where nodes are restarted.
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

#[derive(Clone)]
pub struct RocksDbStorage {
    db: Arc<DB>,
    write_options: Arc<WriteOptions>,
}

impl RocksDbStorage {
    pub fn open(path: &Path, options: RocksDbOptions) -> PatriciaStorageResult<Self> {
        // Check for stale LOCK file from previous process that was killed.
        // RocksDB creates a LOCK file when opening a database. If the process is killed,
        // the LOCK file may remain and prevent opening the database.
        let lock_file = path.join("LOCK");
        if lock_file.exists() {
            // Remove stale lock file to allow opening the database after a forceful kill.
            // RocksDB should handle stale locks, but explicitly removing it ensures we can open.
            if let Err(e) = std::fs::remove_file(&lock_file) {
                // If we can't remove it, RocksDB will try to handle it, but log the issue.
                eprintln!(
                    "Warning: Could not remove stale LOCK file at {}: {}",
                    lock_file.display(),
                    e
                );
            }
        }
        
        // Check if database files exist (CURRENT file is essential for RocksDB).
        // If CURRENT exists, the database should be opened, not created.
        let current_file = path.join("CURRENT");
        let db_should_exist = current_file.exists();
        
        // List files in the directory for debugging
        if path.exists() {
            if let Ok(entries) = std::fs::read_dir(path) {
                let files: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                eprintln!(
                    "RocksDB: Directory {} contains {} files: {:?}",
                    path.display(),
                    files.len(),
                    files
                );
            }
        }
        
        if db_should_exist {
            eprintln!(
                "RocksDB: Opening existing database at {} (CURRENT file exists)",
                path.display()
            );
        } else {
            eprintln!(
                "RocksDB: Database does not exist at {} (CURRENT file missing), will be created",
                path.display()
            );
        }
        
        let db = Arc::new(DB::open(&options.db_options, path)?);
        
        // After opening, verify if this was a new database or existing one
        // by checking if CURRENT file exists now (it should exist after opening)
        let current_exists_after = path.join("CURRENT").exists();
        if db_should_exist && !current_exists_after {
            eprintln!(
                "WARNING: RocksDB may have recreated the database! CURRENT file existed before but not after opening."
            );
        } else if !db_should_exist && current_exists_after {
            eprintln!("RocksDB: Created new database (CURRENT file now exists)");
        } else if db_should_exist && current_exists_after {
            eprintln!("RocksDB: Successfully opened existing database");
        }
        
        let write_options = Arc::new(options.write_options);
        Ok(Self { db, write_options })
    }
}

impl Storage for RocksDbStorage {
    type Stats = NoStats;

    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.db.get(&key.0)?.map(DbValue))
    }

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        Ok(self.db.put_opt(&key.0, &value.0, &self.write_options)?)
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
        Ok(self.db.write_opt(&batch, &self.write_options)?)
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        Ok(self.db.delete_opt(&key.0, &self.write_options)?)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        Some(self.clone())
    }
}
