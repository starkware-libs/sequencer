use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, Storage};

pub type MapStorage = HashMap<DbKey, DbValue>;

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

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
