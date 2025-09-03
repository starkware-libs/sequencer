use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, PatriciaStorageResult, Storage, StorageHashMap};

#[derive(Debug, PartialEq, Serialize)]
pub struct MapStorage(pub StorageHashMap);

impl MapStorage {
    pub fn new() -> Self {
        Self(StorageHashMap::new())
    }
}

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.insert(key, value))
    }

    fn mset(&mut self, key_to_value: StorageHashMap) -> PatriciaStorageResult<()> {
        self.0.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.remove(key))
    }

    fn get(&self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.get(key).cloned())
    }

    fn mget(&self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.0.get(key).cloned()).collect())
    }
}
