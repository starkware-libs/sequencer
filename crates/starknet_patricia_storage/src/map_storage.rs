use std::collections::HashMap;

use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, Storage};

#[derive(Serialize, Debug, Default)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct MapStorage {
    pub storage: HashMap<DbKey, DbValue>,
}

impl Storage for MapStorage {
    fn get(&self, key: &DbKey) -> Option<&DbValue> {
        self.storage.get(key)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue> {
        self.storage.insert(key, value)
    }

    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>> {
        keys.iter().map(|key| self.get(key)).collect::<Vec<_>>()
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
