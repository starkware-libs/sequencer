use std::path::Path;

use rust_rocksdb::{BlockBasedIndexType, BlockBasedOptions, Cache, WriteBatch, WriteOptions, DB};

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};

pub struct RocksdbStorage {
    db: DB,
}

impl RocksdbStorage {
    pub fn open(path: &Path) -> PatriciaStorageResult<Self> {
        use rust_rocksdb::Options;
        let mut opts = Options::default();
        opts.create_if_missing(true);

        opts.set_bytes_per_sync(1024 * 1024);
        // collect more data before writing to disk
        opts.set_write_buffer_size(128 * 1024 * 1024);
        opts.increase_parallelism(8);
        opts.set_max_background_jobs(8);
        // Add more memory buffers over the default of 2
        opts.set_max_write_buffer_number(4);

        let mut block = BlockBasedOptions::default();
        let cache = Cache::new_lru_cache(2 * 1024 * 1024 * 1024); // 2 GiB
        block.set_block_cache(&cache);

        // With a single level filter blocks become too large to sit in cache
        block.set_index_type(BlockBasedIndexType::TwoLevelIndexSearch);
        block.set_partition_filters(true);

        // default block size, consider increasing
        block.set_block_size(4 * 1024);
        block.set_cache_index_and_filter_blocks(true);
        // make sure filter blocks are cached
        block.set_pin_l0_filter_and_index_blocks_in_cache(true);

        block.set_bloom_filter(10.0, false);

        opts.set_block_based_table_factory(&block);

        let db = DB::open(&opts, path).unwrap();
        Ok(Self { db })
    }
}

impl Storage for RocksdbStorage {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let value = self.db.get(&key.0)?;
        Ok(value.map(DbValue))
    }

    fn set(
        &mut self,
        key: DbKey,
        value: crate::storage_trait::DbValue,
    ) -> PatriciaStorageResult<Option<DbValue>> {
        let prev_val = self.db.get(&key.0)?;
        self.db.put(&key.0, &value.0)?;
        Ok(prev_val.map(DbValue))
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut res = Vec::with_capacity(keys.len());
        for key in keys {
            res.push(self.db.get(&key.0)?.map(DbValue));
        }
        Ok(res)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let mut write_options = WriteOptions::default();
        // disable fsync after each write
        write_options.set_sync(false);
        // no write ahead log at all
        write_options.disable_wal(true);

        let mut batch = WriteBatch::default();
        for key in key_to_value.keys() {
            batch.put(&key.0, &key_to_value[key].0);
        }
        self.db.write_opt(&batch, &write_options)?;
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let prev_val = self.db.get(&key.0)?;
        if prev_val.is_some() {
            self.db.delete(&key.0)?;
        }
        Ok(prev_val.map(DbValue))
    }
}
