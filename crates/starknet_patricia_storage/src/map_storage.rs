use std::fmt::Display;
use std::num::NonZeroUsize;

use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{
    DbHashMap,
    DbKey,
    DbValue,
    NoStats,
    PatriciaStorageResult,
    Storage,
    StorageStats,
};

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub DbHashMap);

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    type Stats = NoStats;

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.0.insert(key, value);
        Ok(())
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.0.extend(key_to_value);
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.0.remove(key);
        Ok(())
    }

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.get(key).cloned())
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.0.get(key).cloned()).collect())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached.
pub struct CachedStorage<S: Storage> {
    pub storage: S,
    pub cache: LruCache<DbKey, Option<DbValue>>,
    pub cache_on_write: bool,
    reads: u128,
    cached_reads: u128,
    writes: u128,
}

pub struct CachedStorageConfig {
    // Max number of entries in the cache.
    pub cache_size: NonZeroUsize,

    // If true, the cache is updated on write operations even if the value is not in the cache.
    pub cache_on_write: bool,
}

#[derive(Default)]
pub struct CachedStorageStats<S: StorageStats> {
    pub reads: u128,
    pub cached_reads: u128,
    pub writes: u128,
    pub inner_stats: S,
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
            self.inner_stats.column_values(),
        ]
        .concat()
    }
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(storage: S, config: CachedStorageConfig) -> Self {
        Self {
            storage,
            cache: LruCache::new(config.cache_size),
            cache_on_write: config.cache_on_write,
            reads: 0,
            cached_reads: 0,
            writes: 0,
        }
    }

    fn update_cached_value(&mut self, key: &DbKey, value: &DbValue) {
        if self.cache_on_write || self.cache.contains(key) {
            self.cache.put(key.clone(), Some(value.clone()));
        }
    }

    pub fn total_reads(&self) -> u128 {
        self.reads
    }

    pub fn total_cached_reads(&self) -> u128 {
        self.cached_reads
    }

    pub fn total_writes(&self) -> u128 {
        self.writes
    }
}

impl<S: Storage> Storage for CachedStorage<S> {
    type Stats = CachedStorageStats<S::Stats>;

    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        self.reads += 1;
        if let Some(cached_value) = self.cache.get(key) {
            self.cached_reads += 1;
            return Ok(cached_value.clone());
        }

        let storage_value = self.storage.get(key)?;
        self.cache.put(key.clone(), storage_value.clone());
        Ok(storage_value)
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.writes += 1;
        self.storage.set(key.clone(), value.clone())?;
        self.update_cached_value(&key, &value);
        Ok(())
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut values = vec![None; keys.len()]; // The None values are placeholders.
        let mut keys_to_fetch = Vec::new();
        let mut indices_to_fetch = Vec::new();

        for (index, key) in keys.iter().enumerate() {
            if let Some(cached_value) = self.cache.get(key) {
                values[index] = cached_value.clone();
            } else {
                keys_to_fetch.push(*key);
                indices_to_fetch.push(index);
            }
        }

        self.reads += u128::try_from(keys.len()).expect("usize should fit in u128");
        self.cached_reads +=
            u128::try_from(keys.len() - keys_to_fetch.len()).expect("usize should fit in u128");

        let fetched_values = self.storage.mget(keys_to_fetch.as_slice())?;
        indices_to_fetch.iter().zip(keys_to_fetch).zip(fetched_values).for_each(
            |((index, key), value)| {
                self.cache.put((*key).clone(), value.clone());
                values[*index] = value;
            },
        );

        Ok(values)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.writes += u128::try_from(key_to_value.len()).expect("usize should fit in u128");
        self.storage.mset(key_to_value.clone())?;
        key_to_value.iter().for_each(|(key, value)| {
            self.update_cached_value(key, value);
        });
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.cache.pop(key);
        self.storage.delete(key)
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(CachedStorageStats {
            reads: self.reads,
            cached_reads: self.cached_reads,
            writes: self.writes,
            inner_stats: self.storage.get_stats()?,
        })
    }
}
