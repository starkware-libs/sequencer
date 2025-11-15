use std::fmt::Display;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    NoStats,
    PatriciaStorageError,
    PatriciaStorageResult,
    Storage,
    StorageStats,
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct MapStorage(Arc<RwLock<DbHashMap>>);

impl MapStorage {
    pub fn new(initial_map: DbHashMap) -> Self {
        Self(Arc::new(RwLock::new(initial_map)))
    }

    fn read_lock<'a>(&'a self) -> PatriciaStorageResult<RwLockReadGuard<'a, DbHashMap>> {
        self.0.read().map_err(|e| PatriciaStorageError::PoisonedLock(e.to_string()))
    }

    fn write_lock<'a>(&'a self) -> PatriciaStorageResult<RwLockWriteGuard<'a, DbHashMap>> {
        self.0.write().map_err(|e| PatriciaStorageError::PoisonedLock(e.to_string()))
    }

    pub fn setnx(
        &mut self,
        db_name: &str,
        key: DbKey,
        value: DbValue,
    ) -> PatriciaStorageResult<()> {
        let mut write_locked = self.write_lock()?;
        if let Some(old_value) = write_locked.get(&key) {
            return Err(PatriciaStorageError::KeyAlreadySet {
                db_name: db_name.to_string(),
                key,
                old_value: old_value.clone(),
                new_value: value,
            });
        }
        write_locked.insert(key, value);
        Ok(())
    }

    pub fn len(&self) -> PatriciaStorageResult<usize> {
        Ok(self.read_lock()?.len())
    }

    pub fn is_empty(&self) -> PatriciaStorageResult<bool> {
        Ok(self.read_lock()?.is_empty())
    }

    pub fn cloned_map(&self) -> PatriciaStorageResult<DbHashMap> {
        Ok(self.read_lock()?.clone())
    }
}

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    type Stats = NoStats;

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.write_lock()?.insert(key, value);
        Ok(())
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.write_lock()?.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.write_lock()?.remove(key);
        Ok(())
    }

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.read_lock()?.get(key).cloned())
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let read_locked = self.read_lock()?;
        Ok(keys.iter().map(|key| read_locked.get(key).cloned()).collect())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}

struct StorageAndCache<S: Storage> {
    storage: S,
    cache: LruCache<DbKey, Option<DbValue>>,
    reads: u128,
    cached_reads: u128,
    writes: u128,
}

impl<S: Storage> StorageAndCache<S> {
    fn update_cached_value(&mut self, cache_on_write: bool, key: &DbKey, value: &DbValue) {
        if cache_on_write || self.cache.contains(key) {
            self.cache.put(key.clone(), Some(value.clone()));
        }
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached, unless `cache_on_write` is true.
///
/// As the cache is updated on both write and read operations, truly concurrent read access is not
/// possible while using CachedStorage. That being said, this storage can be safely cloned and
/// shared between threads.
#[derive(Clone)]
pub struct CachedStorage<S: Storage> {
    storage_and_cache: Arc<RwLock<StorageAndCache<S>>>,
    cache_on_write: bool,
    include_inner_stats: bool,
}

pub struct CachedStorageConfig {
    // Max number of entries in the cache.
    pub cache_size: NonZeroUsize,

    // If true, the cache is updated on write operations even if the value is not in the cache.
    pub cache_on_write: bool,

    // If true, the inner stats are included when collecting statistics.
    pub include_inner_stats: bool,
}

#[derive(Default)]
pub struct CachedStorageStats<S: StorageStats> {
    pub reads: u128,
    pub cached_reads: u128,
    pub writes: u128,
    pub inner_stats: Option<S>,
}

impl<S: StorageStats> CachedStorageStats<S> {
    fn cache_hit_rate(&self) -> f64 {
        #[allow(clippy::as_conversions)]
        let ratio = self.cached_reads as f64 / self.reads as f64;
        ratio
    }
}

impl<S: StorageStats> Display for CachedStorageStats<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CachedStorageStats: {}", self.stat_string())
    }
}

impl<S: StorageStats> StorageStats for CachedStorageStats<S> {
    fn column_titles() -> Vec<&'static str> {
        [vec!["reads", "cached reads", "writes", "cache hit rate"], S::column_titles()].concat()
    }

    fn column_values(&self) -> Vec<String> {
        [
            vec![
                self.reads.to_string(),
                self.cached_reads.to_string(),
                self.writes.to_string(),
                self.cache_hit_rate().to_string(),
            ],
            self.inner_stats.as_ref().map(|s| s.column_values()).unwrap_or_default(),
        ]
        .concat()
    }
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(storage: S, config: CachedStorageConfig) -> Self {
        Self {
            storage_and_cache: Arc::new(RwLock::new(StorageAndCache {
                storage,
                cache: LruCache::new(config.cache_size),
                reads: 0,
                cached_reads: 0,
                writes: 0,
            })),
            cache_on_write: config.cache_on_write,
            include_inner_stats: config.include_inner_stats,
        }
    }

    fn read_lock<'a>(&'a self) -> PatriciaStorageResult<RwLockReadGuard<'a, StorageAndCache<S>>> {
        self.storage_and_cache.read().map_err(|e| PatriciaStorageError::PoisonedLock(e.to_string()))
    }

    fn write_lock<'a>(&'a self) -> PatriciaStorageResult<RwLockWriteGuard<'a, StorageAndCache<S>>> {
        self.storage_and_cache
            .write()
            .map_err(|e| PatriciaStorageError::PoisonedLock(e.to_string()))
    }
}

impl<S: Storage> Storage for CachedStorage<S> {
    type Stats = CachedStorageStats<S::Stats>;

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let mut storage_and_cache = self.write_lock()?;
        storage_and_cache.reads += 1;
        if let Some(cached_value) = storage_and_cache.cache.get(key).cloned() {
            storage_and_cache.cached_reads += 1;
            return Ok(cached_value);
        }

        let storage_value = storage_and_cache.storage.get(key)?;
        storage_and_cache.cache.put(key.clone(), storage_value.clone());
        Ok(storage_value)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        let mut storage_and_cache = self.write_lock()?;
        storage_and_cache.writes += 1;
        storage_and_cache.storage.set(key.clone(), value.clone())?;
        storage_and_cache.update_cached_value(self.cache_on_write, &key, &value);
        Ok(())
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut storage_and_cache = self.write_lock()?;
        let mut values = vec![None; keys.len()]; // The None values are placeholders.
        let mut keys_to_fetch = Vec::new();
        let mut indices_to_fetch = Vec::new();

        for (index, key) in keys.iter().enumerate() {
            if let Some(cached_value) = storage_and_cache.cache.get(key) {
                values[index] = cached_value.clone();
            } else {
                keys_to_fetch.push(*key);
                indices_to_fetch.push(index);
            }
        }

        storage_and_cache.reads += u128::try_from(keys.len()).expect("usize should fit in u128");
        storage_and_cache.cached_reads +=
            u128::try_from(keys.len() - keys_to_fetch.len()).expect("usize should fit in u128");

        let fetched_values = storage_and_cache.storage.mget(keys_to_fetch.as_slice())?;
        indices_to_fetch.iter().zip(keys_to_fetch).zip(fetched_values).for_each(
            |((index, key), value)| {
                storage_and_cache.cache.put((*key).clone(), value.clone());
                values[*index] = value;
            },
        );

        Ok(values)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let mut storage_and_cache = self.write_lock()?;
        storage_and_cache.writes +=
            u128::try_from(key_to_value.len()).expect("usize should fit in u128");
        storage_and_cache.storage.mset(key_to_value.clone())?;
        key_to_value.iter().for_each(|(key, value)| {
            storage_and_cache.update_cached_value(self.cache_on_write, key, value);
        });
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        let mut storage_and_cache = self.write_lock()?;
        storage_and_cache.cache.pop(key);
        storage_and_cache.storage.delete(key)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        let storage_and_cache = self.read_lock()?;
        Ok(CachedStorageStats {
            reads: storage_and_cache.reads,
            cached_reads: storage_and_cache.cached_reads,
            writes: storage_and_cache.writes,
            inner_stats: if self.include_inner_stats {
                Some(storage_and_cache.storage.get_stats()?)
            } else {
                None
            },
        })
    }

    fn reset_stats(&mut self) -> PatriciaStorageResult<()> {
        let mut storage_and_cache = self.write_lock()?;
        storage_and_cache.reads = 0;
        storage_and_cache.cached_reads = 0;
        storage_and_cache.writes = 0;
        storage_and_cache.storage.reset_stats()
    }
}
