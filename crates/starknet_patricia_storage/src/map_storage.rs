use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, PatriciaStorageResult, Storage};

pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.insert(key, value))
    }

    fn mset(&mut self, key_to_value: MapStorage) -> PatriciaStorageResult<()> {
        self.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.remove(key))
    }

    fn get(&self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.get(key).cloned())
    }

    fn mget(&self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.get(key).cloned()).collect())
    }
}
