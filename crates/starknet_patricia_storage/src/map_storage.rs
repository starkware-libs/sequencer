use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbStorageKey, DbStorageValue, Storage};

#[derive(Serialize, Debug, Default)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct MapStorage {
    pub storage: HashMap<DbStorageKey, DbStorageValue>,
}

impl Storage for MapStorage {
    fn get(&self, key: &DbStorageKey) -> Option<&DbStorageValue> {
        self.storage.get(key)
    }

    fn set(&mut self, key: DbStorageKey, value: DbStorageValue) -> Option<DbStorageValue> {
        self.storage.insert(key, value)
    }

    fn mget(&self, keys: &[DbStorageKey]) -> Vec<Option<&DbStorageValue>> {
        keys.iter().map(|key| self.get(key)).collect::<Vec<_>>()
    }

    fn mset(&mut self, key_to_value: HashMap<DbStorageKey, DbStorageValue>) {
        self.storage.extend(key_to_value);
    }

    fn delete(&mut self, key: &DbStorageKey) -> Option<DbStorageValue> {
        self.storage.remove(key)
    }
}

impl From<HashMap<DbStorageKey, DbStorageValue>> for MapStorage {
    fn from(storage: HashMap<DbStorageKey, DbStorageValue>) -> Self {
        Self { storage }
    }
}
