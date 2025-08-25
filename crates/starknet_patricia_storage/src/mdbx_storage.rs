use std::path::Path;

use libmdbx::{Database as MdbxDb, TableFlags, WriteFlags, WriteMap};

use crate::map_storage::MapStorage;
use crate::storage_trait::{DbKey, DbValue, PatriciaStorageResult, Storage};

pub struct MdbxStorage {
    db: MdbxDb<WriteMap>,
}

impl MdbxStorage {
    pub fn open(path: &Path) -> PatriciaStorageResult<Self> {
        let db = MdbxDb::<WriteMap>::new().open(path)?;
        let txn = db.begin_rw_txn()?;
        txn.create_table(None, TableFlags::empty())?;
        txn.commit()?;
        Ok(Self { db })
    }
}

impl Storage for MdbxStorage {
    fn get(&self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        Ok(txn.get(&table, &key.0)?.map(DbValue))
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        let prev_val = txn.get(&table, &key.0)?.map(DbValue);
        txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        txn.commit()?;
        Ok(prev_val)
    }

    fn mget(&self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        let mut res = Vec::with_capacity(keys.len());
        for key in keys {
            res.push(txn.get(&table, &key.0)?.map(DbValue));
        }
        Ok(res)
    }

    fn mset(&mut self, key_to_value: MapStorage) -> PatriciaStorageResult<()> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        for (key, value) in key_to_value {
            txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        }
        txn.commit()?;
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        let prev_val = txn.get(&table, &key.0)?.map(DbValue);
        if prev_val.is_some() {
            txn.del(&table, &key.0, None)?;
        }
        txn.commit()?;
        Ok(prev_val)
    }
}
