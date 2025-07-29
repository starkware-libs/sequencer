use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, ReadOnlyStorage, Storage};

#[derive(Serialize, Debug, Default)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct MapStorage {
    pub storage: HashMap<DbKey, DbValue>,
}

pub struct ReadOnlyMapStorage<'a> {
    pub storage: &'a HashMap<DbKey, DbValue>,
}

impl ReadOnlyStorage for ReadOnlyMapStorage<'_> {
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect()
    }
}

impl ReadOnlyStorage for MapStorage {
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect()
    }
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.storage.insert(key, value)
    }

    fn mset(&mut self, key_to_value: HashMap<DbKey, DbValue>) {
        self.storage.extend(key_to_value);
    }

    fn delete(&mut self, key: &DbKey) -> Option<DbValue> {
        self.storage.remove(key)
    }
}

impl From<HashMap<DbKey, DbValue>> for MapStorage {
    fn from(storage: HashMap<DbKey, DbValue>) -> Self {
        Self { storage }
    }
}
