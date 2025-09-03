use std::num::NonZeroUsize;

use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub DbHashMap);

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.insert(key, value))
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.0.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.remove(key))
    }

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.get(key).cloned())
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.0.get(key).cloned()).collect())
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached.
pub struct CachedStorage<S: Storage> {
    pub storage: S,
    pub cache: LruCache<DbKey, Option<DbValue>>,
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(storage: S, cache_capacity: NonZeroUsize) -> Self {
        Self { storage, cache: LruCache::new(cache_capacity) }
    }

    fn update_cached_value(&mut self, key: &DbKey, value: &DbValue) {
        if self.cache.contains(key) {
            self.cache.put(key.clone(), Some(value.clone()));
        }
    }
}

impl<S: Storage> Storage for CachedStorage<S> {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        if let Some(cached_value) = self.cache.get(key) {
            return Ok(cached_value.clone());
        }

        let storage_value = self.storage.get(key)?;
        self.cache.put(key.clone(), storage_value.clone());
        Ok(storage_value)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        self.update_cached_value(&key, &value);
        self.storage.set(key, value)
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut values = vec![None; keys.len()]; // The None values are placeholder.
        let mut keys_to_fetch = Vec::new();
        let mut indices_to_fetch = Vec::new();

        for (index, key) in keys.into_iter().enumerate() {
            if let Some(cached_value) = self.cache.get(key) {
                values[index] = cached_value.clone();
            } else {
                keys_to_fetch.push(*key);
                indices_to_fetch.push(index);
            }
        }

        let fetched_values = self.storage.mget(keys_to_fetch.as_slice())?;
        indices_to_fetch
            .into_iter()
            .zip(keys_to_fetch.into_iter())
            .zip(fetched_values.into_iter())
            .for_each(|((index, key), value)| {
                self.cache.put(key.clone(), value.clone());
                values[index] = value;
            });

        Ok(values)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
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
