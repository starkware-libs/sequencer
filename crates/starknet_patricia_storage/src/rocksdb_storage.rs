use std::path::Path;

use rust_rocksdb::{WriteBatch, WriteOptions, DB};

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};


pub struct RocksdbStorage {
    db: DB,
}

impl RocksdbStorage {
    pub fn open(path: &Path) -> PatriciaStorageResult<Self> {
        let db = DB::open_default(path).unwrap();
        Ok(Self { db })
    }
}

impl Storage for RocksdbStorage {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let value = self.db.get(&key.0)?;
        Ok(value.map(DbValue))
    }

    fn set(&mut self, key: DbKey, value: crate::storage_trait::DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        let prev_val = self.db.get(&key.0)?;
        self.db.put(&key.0, &value.0)?;
        Ok(prev_val.map(DbValue))
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut res = Vec::with_capacity(keys.len());
        for key in keys {
            res.push(self.db.get(&key.0)?.map(DbValue));
        };
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