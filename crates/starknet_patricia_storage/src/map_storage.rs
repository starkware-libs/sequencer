use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, PatriciaStorageError, Storage};

pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> Result<Option<DbValue>, PatriciaStorageError> {
        Ok(self.insert(key, value))
    }

    fn mset(&mut self, key_to_value: MapStorage) -> Result<(), PatriciaStorageError> {
        self.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> Result<Option<DbValue>, PatriciaStorageError> {
        Ok(self.remove(key))
    }

    fn get(&self, key: &DbKey) -> Result<Option<DbValue>, PatriciaStorageError> {
        Ok(self.get(key).cloned())
    }

    fn mget(&self, keys: &[DbKey]) -> Result<Vec<Option<DbValue>>, PatriciaStorageError> {
        Ok(keys.iter().map(|key| self.get(key).cloned()).collect())
    }
}
