use std::collections::HashMap;
use std::fmt::Display;
use std::num::NonZeroUsize;

use itertools::Itertools;
use lru::LruCache;
use serde::Serialize;

use crate::storage_trait::{
    BlockNumber,
    DbHashMap,
    DbValue,
    NoStats,
    PatriciaStorageError,
    PatriciaStorageResult,
    Storage,
    StorageStats,
    TrieKey,
};

#[derive(Debug, PartialEq, Serialize, Eq, Default, Hash)]
pub enum MapStorageLayer {
    #[default]
    LatestTrie,
    HistoricalTries(BlockNumber),
}
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub HashMap<MapStorageLayer, DbHashMap>);

impl MapStorage {
    pub fn new() -> Self {
        let mut layers = HashMap::new();
        layers.insert(MapStorageLayer::LatestTrie, HashMap::new());
        Self(layers)
    }
}

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

trait MapStorageKey {
    fn get_timestamp(&self) -> Option<BlockNumber>;
}

impl MapStorageKey for TrieKey {
    fn get_timestamp(&self) -> Option<BlockNumber> {
        match self {
            TrieKey::HistoricalTries(_, block_number) => Some(*block_number),
            TrieKey::LatestTrie(_) => None,
        }
    }
}

impl Storage for MapStorage {
    type Stats = NoStats;

    fn set(&mut self, key: TrieKey, value: DbValue) -> PatriciaStorageResult<()> {
        if let Some(timestamp) = key.get_timestamp() {
            let layer = MapStorageLayer::HistoricalTries(timestamp);
            if let Some(layer) = self.0.get_mut(&layer) {
                layer.insert(key, value);
            } else {
                let mut layer_content = HashMap::new();
                layer_content.insert(key, value);
                self.0.insert(layer, layer_content);
            }
        } else {
            self.0.get_mut(&MapStorageLayer::LatestTrie).unwrap().insert(key, value);
        }

        Ok(())
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let mut timestamps = key_to_value.keys().map(|k| k.get_timestamp());
        let timestamp =
            timestamps.all_equal_value().map_err(|_| PatriciaStorageError::MultipleTimestamps)?;

        if let Some(timestamp) = timestamp {
            let layer = MapStorageLayer::HistoricalTries(timestamp);
            if let Some(layer) = self.0.get_mut(&layer) {
                layer.extend(key_to_value);
            } else {
                let mut layer_content = HashMap::new();
                layer_content.extend(key_to_value);
                self.0.insert(layer, layer_content);
            }
        } else {
            self.0.get_mut(&MapStorageLayer::LatestTrie).unwrap().extend(key_to_value);
        }

        Ok(())
    }

    fn delete(&mut self, key: &TrieKey) -> PatriciaStorageResult<()> {
        let timestamp = key.get_timestamp();
        if timestamp.is_some() {
            return Err(PatriciaStorageError::AttemptToModifyHistory);
        }
        self.0.get_mut(&MapStorageLayer::LatestTrie).unwrap().remove(key);
        Ok(())
    }

    fn get(&mut self, key: &TrieKey) -> PatriciaStorageResult<Option<DbValue>> {
        let timestamp = key.get_timestamp();
        if let Some(timestamp) = timestamp {
            for i in (0..(timestamp.0 + 1)).rev() {
                if let Some(layer) = self.0.get(&MapStorageLayer::HistoricalTries(BlockNumber(i))) {
                    let value = layer.get(key);
                    if value.is_some() {
                        return Ok(value.cloned());
                    }
                }
            }
            Ok(None)
        } else {
            Ok(self.0.get(&MapStorageLayer::LatestTrie).unwrap().get(key).cloned())
        }
    }

    fn mget(&mut self, keys: &[&TrieKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut timestamps = keys.iter().map(|k| k.get_timestamp());
        if !timestamps.all_equal() {
            return Err(PatriciaStorageError::MultipleTimestamps);
        };

        keys.iter().map(|key| self.get(key)).collect()
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached.
pub struct CachedStorage<S: Storage> {
    pub storage: S,
    pub cache: LruCache<TrieKey, Option<DbValue>>,
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

    fn update_cached_value(&mut self, key: &TrieKey, value: &DbValue) {
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

    fn get(&mut self, key: &TrieKey) -> PatriciaStorageResult<Option<DbValue>> {
        self.reads += 1;
        if let Some(cached_value) = self.cache.get(key) {
            self.cached_reads += 1;
            return Ok(cached_value.clone());
        }

        let storage_value = self.storage.get(key)?;
        self.cache.put(key.clone(), storage_value.clone());
        Ok(storage_value)
    }

    fn set(&mut self, key: TrieKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.writes += 1;
        self.storage.set(key.clone(), value.clone())?;
        self.update_cached_value(&key, &value);
        Ok(())
    }

    fn mget(&mut self, keys: &[&TrieKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
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

    fn delete(&mut self, key: &TrieKey) -> PatriciaStorageResult<()> {
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
