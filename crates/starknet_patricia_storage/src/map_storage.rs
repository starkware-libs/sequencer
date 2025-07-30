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
        get_for_map(self.storage, key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        mget_for_map(self.storage, keys)
    }
}

impl ReadOnlyStorage for MapStorage {
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        get_for_map(&self.storage, key)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        mget_for_map(&self.storage, keys)
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

fn mget_for_map<'a>(map: &'a HashMap<DbKey, DbValue>, keys: &[DbKey]) -> Vec<Option<&'a DbValue>> {
    keys.iter().map(|key| map.get(key)).collect()
}

fn get_for_map<'a>(map: &'a HashMap<DbKey, DbValue>, key: &DbKey) -> Option<&'a DbValue> {
    map.get(key)
}
