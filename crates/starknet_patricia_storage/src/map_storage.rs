use std::collections::HashMap;

use serde::Serialize;

use crate::storage::storage_trait::{Storage, StorageKey, StorageValue};

#[derive(Serialize, Debug, Default)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct MapStorage {
    pub storage: HashMap<StorageKey, StorageValue>,
}

impl Storage for MapStorage {
    fn get(&self, key: &StorageKey) -> Option<&StorageValue> {
        self.storage.get(key)
    }

    fn set(&mut self, key: StorageKey, value: StorageValue) -> Option<StorageValue> {
        self.storage.insert(key, value)
    }

    fn mget(&self, keys: &[StorageKey]) -> Vec<Option<&StorageValue>> {
        keys.iter().map(|key| self.get(key)).collect::<Vec<_>>()
    }

    fn mset(&mut self, key_to_value: HashMap<StorageKey, StorageValue>) {
        self.storage.extend(key_to_value);
    }

    fn delete(&mut self, key: &StorageKey) -> Option<StorageValue> {
        self.storage.remove(key)
    }
}

impl From<HashMap<StorageKey, StorageValue>> for MapStorage {
    fn from(storage: HashMap<StorageKey, StorageValue>) -> Self {
        Self { storage }
    }
}
