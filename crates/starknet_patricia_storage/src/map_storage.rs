use std::collections::BTreeMap;
use std::fmt::Display;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationErrors};

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbValue,
    EmptyStorageConfig,
    NoStats,
    NullStorage,
    PatriciaStorageResult,
    Storage,
    StorageConfigTrait,
    StorageStats,
};

// 1M entries.
const DEFAULT_CACHE_SIZE: usize = 1000000;

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MapStorage(pub DbHashMap);

#[derive(Serialize, Debug)]
pub struct BorrowedStorage<'a, S: Storage> {
    pub storage: &'a mut S,
}

impl Storage for MapStorage {
    type Stats = NoStats;
    type Config = EmptyStorageConfig;

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.0.insert(key, value);
        Ok(())
    }

    async fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.0.extend(key_to_value);
        Ok(())
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.0.remove(key);
        Ok(())
    }

    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(self.0.get(key).cloned())
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(keys.iter().map(|key| self.0.get(key).cloned()).collect())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        // Need a concrete Option type.
        None::<NullStorage>
    }
}

/// A storage wrapper that adds an LRU cache to an underlying storage.
/// Only getter methods are cached.
pub struct CachedStorage<S: Storage> {
    pub storage: S,
    pub cache: LruCache<DbKey, Option<DbValue>>,
    pub cache_on_write: bool,
    reads: AtomicU64,
    cached_reads: AtomicU64,
    writes: u128,
    include_inner_stats: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CachedStorageConfig<InnerStorageConfig: StorageConfigTrait> {
    // Max number of entries in the cache.
    pub cache_size: NonZeroUsize,

    // If true, the cache is updated on write operations even if the value is not in the cache.
    pub cache_on_write: bool,

    // If true, the inner stats are included when collecting statistics.
    pub include_inner_stats: bool,

    // The config of the underlying storage.
    pub inner_storage_config: InnerStorageConfig,
}

impl<InnerStorageConfig: StorageConfigTrait> Default for CachedStorageConfig<InnerStorageConfig> {
    fn default() -> Self {
        Self {
            cache_size: NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap(),
            cache_on_write: true,
            include_inner_stats: true,
            inner_storage_config: InnerStorageConfig::default(),
        }
    }
}

impl<InnerStorageConfig: StorageConfigTrait> Validate for CachedStorageConfig<InnerStorageConfig> {
    fn validate(&self) -> Result<(), ValidationErrors> {
        self.inner_storage_config.validate()
    }
}

impl<InnerStorageConfig: StorageConfigTrait> SerializeConfig
    for CachedStorageConfig<InnerStorageConfig>
{
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut cached_storage_config = BTreeMap::from([
            ser_param(
                "cache_size",
                &self.cache_size,
                "Max number of entries in the cache",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "cache_on_write",
                &self.cache_on_write,
                "If true, the cache is updated on write operations",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "include_inner_stats",
                &self.include_inner_stats,
                "If true, the inner stats are included when collecting statistics",
                ParamPrivacyInput::Public,
            ),
        ]);

        cached_storage_config.extend(prepend_sub_config_name(
            self.inner_storage_config.dump(),
            "inner_storage_config",
        ));

        cached_storage_config
    }
}

impl<InnerStorageConfig: StorageConfigTrait> StorageConfigTrait
    for CachedStorageConfig<InnerStorageConfig>
{
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
            self.inner_stats.as_ref().map(|s| s.column_values()).unwrap_or(vec![
                "".to_string();
                S::column_titles()
                    .len()
            ]),
        ]
        .concat()
    }
}

impl<S: Storage> CachedStorage<S> {
    pub fn new(storage: S, config: CachedStorageConfig<S::Config>) -> Self {
        Self {
            storage,
            cache: LruCache::new(config.cache_size),
            cache_on_write: config.cache_on_write,
            reads: AtomicU64::new(0),
            cached_reads: AtomicU64::new(0),
            writes: 0,
            include_inner_stats: config.include_inner_stats,
        }
    }

    fn update_cached_value(&mut self, key: &DbKey, value: &DbValue) {
        if self.cache_on_write || self.cache.contains(key) {
            self.cache.put(key.clone(), Some(value.clone()));
        }
    }

    pub fn total_writes(&self) -> u128 {
        self.writes
    }
}

impl<S: Storage> Storage for CachedStorage<S> {
    type Stats = CachedStorageStats<S::Stats>;
    type Config = CachedStorageConfig<S::Config>;

    async fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        if let Some(cached_value) = self.cache.get(key) {
            self.cached_reads.fetch_add(1, Ordering::Relaxed);
            return Ok(cached_value.clone());
        }

        self.reads.fetch_add(1, Ordering::Relaxed);
        let storage_value = self.storage.get(key).await?;
        self.cache.put(key.clone(), storage_value.clone());
        Ok(storage_value)
    }

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        self.writes += 1;
        self.storage.set(key.clone(), value.clone()).await?;
        self.update_cached_value(&key, &value);
        Ok(())
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let mut values = vec![None; keys.len()]; // The None values are placeholders.
        let mut keys_to_fetch = Vec::new();
        let mut indices_to_fetch = Vec::new();
        let mut cached_reads = 0;

        for (index, key) in keys.iter().enumerate() {
            if let Some(cached_value) = self.cache.get(key) {
                values[index] = cached_value.clone();
                cached_reads += 1;
            } else {
                keys_to_fetch.push(*key);
                indices_to_fetch.push(index);
            }
        }

        let n_keys = u64::try_from(keys.len()).expect("keys length should fit in u64");
        self.reads.fetch_add(n_keys, Ordering::Relaxed);
        self.cached_reads.fetch_add(cached_reads, Ordering::Relaxed);

        let fetched_values = self.storage.mget(keys_to_fetch.as_slice()).await?;
        indices_to_fetch.iter().zip(keys_to_fetch).zip(fetched_values).for_each(
            |((index, key), value)| {
                self.cache.put((*key).clone(), value.clone());
                values[*index] = value;
            },
        );

        Ok(values)
    }

    async fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        self.writes += u128::try_from(key_to_value.len()).expect("usize should fit in u128");
        self.storage.mset(key_to_value.clone()).await?;
        key_to_value.iter().for_each(|(key, value)| {
            self.update_cached_value(key, value);
        });
        Ok(())
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        self.cache.pop(key);
        self.storage.delete(key).await
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        let reads = u128::from(self.reads.load(Ordering::Relaxed));
        let cached_reads = u128::from(self.cached_reads.load(Ordering::Relaxed));
        Ok(CachedStorageStats {
            reads,
            cached_reads,
            writes: self.writes,
            inner_stats: if self.include_inner_stats {
                Some(self.storage.get_stats()?)
            } else {
                None
            },
        })
    }

    fn reset_stats(&mut self) -> PatriciaStorageResult<()> {
        self.reads.store(0, Ordering::Relaxed);
        self.cached_reads.store(0, Ordering::Relaxed);
        self.writes = 0;
        self.storage.reset_stats()
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        // Need a concrete Option type.
        None::<NullStorage>
    }
}
