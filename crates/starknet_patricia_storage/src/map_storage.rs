use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, Storage};

pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

pub type BorrowedMapStorage<'a> = BorrowedStorage<'a, MapStorage>;

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.insert(key, value)
    }

    fn mset(&mut self, key_to_value: MapStorage) {
        self.extend(key_to_value);
    }

    fn delete(&mut self, key: &DbKey) -> Option<DbValue> {
        self.remove(key)
    }

    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect()
    }
}

impl<S: Storage> Storage for BorrowedStorage<'_, S> {
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.storage.set(key, value)
    }

    fn mset(&mut self, key_to_value: MapStorage) {
        self.storage.mset(key_to_value);
    }

    fn delete(&mut self, key: &DbKey) -> Option<DbValue> {
        self.storage.delete(key)
    }

    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        self.storage.mget(keys)
    }
}
