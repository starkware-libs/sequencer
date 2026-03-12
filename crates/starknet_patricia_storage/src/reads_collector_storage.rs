use crate::storage_trait::{DbHashMap, DbKey, DbValue};

// TODO(Nimrod): Explain more about the dangerous API once it's merged.
/// A collection of key-value pairs read from storage.
/// Used to accumulate reads across concurrent tasks and merge them back into a single storage.
/// It's important that the inner map can only be modified via private methods in this module,
/// otherwise the storage trait can expose dangerous API.
#[derive(Default)]
pub struct StorageReads(DbHashMap);

impl StorageReads {
    pub fn new() -> Self {
        Self(DbHashMap::new())
    }

    // This method must be private, see struct documentation.
    #[allow(dead_code)]
    fn insert(&mut self, key: DbKey, value: DbValue) {
        self.0.insert(key, value);
    }

    pub fn extend(&mut self, other: StorageReads) {
        self.0.extend(other.0);
    }
}
