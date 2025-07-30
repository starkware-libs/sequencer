use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, Storage};
pub type MapStorage = HashMap<DbKey, DbValue>;
#[derive(Serialize, Debug)]
pub struct BorrowedMapStorage<'a> {
    pub storage: &'a mut MapStorage,
}

impl Storage for BorrowedMapStorage<'_> {
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.storage.insert(key, value)
    }

    fn mset(&mut self, key_to_value: MapStorage) {
        self.storage.extend(key_to_value);
    }

    fn delete(&mut self, key: &DbKey) -> Option<DbValue> {
        self.storage.remove(key)
    }

    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.storage.get(key)).collect()
    }
}
