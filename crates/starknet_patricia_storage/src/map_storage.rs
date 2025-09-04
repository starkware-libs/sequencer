use std::num::NonZeroUsize;

use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, PatriciaStorageResult, Storage, StorageHashMap};

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub StorageHashMap);

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.insert(key, value))
    }

    fn mset(&mut self, key_to_value: StorageHashMap) -> PatriciaStorageResult<()> {
        self.0.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.remove(key))
    }

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.get(key).cloned())
    }

    fn mget(&mut self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.0.get(key).cloned()).collect())
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached.
pub struct CachedStorage<S: Storage> {
    pub storage: S,
    pub cache: LruCache<DbKey, DbValue>,
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(storage: S, cache_capacity: NonZeroUsize) -> Self {
        Self { storage, cache: LruCache::new(cache_capacity) }
    }

    fn update_cached_value(&mut self, key: &DbKey, value: &DbValue) {
        if self.cache.contains(key) {
            self.cache.put(key.clone(), value.clone());
        }
    }

    fn maybe_put_value(&mut self, key: &DbKey, option_value: &Option<DbValue>) {
        if let Some(value) = option_value {
            self.cache.put(key.clone(), value.clone());
        }
    }
}

impl<S: Storage> Storage for CachedStorage<S> {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        if let Some(cached_value) = self.cache.get(key) {
            return Ok(Some(cached_value.clone()));
        }

        let storage_value = self.storage.get(key)?;
        self.maybe_put_value(&key, &storage_value);

        Ok(storage_value)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        self.update_cached_value(&key, &value);
        self.storage.set(key, value)
    }

    fn mget(&mut self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let values = self.storage.mget(keys)?;
        keys.iter().zip(&values).for_each(|(key, value)| {
            self.maybe_put_value(key, value);
        });
        Ok(values)
    }

    fn mset(&mut self, key_to_value: StorageHashMap) -> PatriciaStorageResult<()> {
        key_to_value.iter().for_each(|(key, value)| {
            self.update_cached_value(key, value);
        });
        self.storage.mset(key_to_value)
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        self.cache.pop(key);
        self.storage.delete(key)
    }
}
