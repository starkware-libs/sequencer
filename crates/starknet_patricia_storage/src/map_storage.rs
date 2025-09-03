use std::num::NonZeroUsize;

use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{DbKey, DbValue, PatriciaStorageResult, Storage, StorageHashMap};

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub StorageHashMap);

impl MapStorage {
    pub fn new() -> Self {
        Self(StorageHashMap::new())
    }
}

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
}

impl<S: Storage> Storage for CachedStorage<S> {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        if let Some(cached_value) = self.cache.get(key) {
            return Ok(Some(cached_value.clone()));
        }

        let storage_value = self.storage.get(key)?;
        if let Some(value) = &storage_value {
            self.cache.put(key.clone(), value.clone());
        }

        Ok(storage_value)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        self.cache.pop(&key);
        self.storage.set(key, value)
    }

    fn mget(&mut self, keys: &[DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        keys.iter().map(|key| self.get(key)).collect::<PatriciaStorageResult<Vec<_>>>()
    }

    fn mset(&mut self, key_to_value: StorageHashMap) -> PatriciaStorageResult<()> {
        key_to_value.keys().for_each(|key| {
            self.cache.pop(&key);
        });
        self.storage.mset(key_to_value)
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        self.cache.pop(key);
        self.storage.delete(key)
    }
}
