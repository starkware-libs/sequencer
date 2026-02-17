use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Debug, Display};
use std::future::Future;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize, Serializer};
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::errors::DeserializationError;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DbKey(pub Vec<u8>);

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct DbValue(pub Vec<u8>);

impl Debug for DbValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DbValue(0x{})", hex::encode(&self.0))
    }
}

pub type DbHashMap = HashMap<DbKey, DbValue>;

/// An error that can occur when interacting with the database.
#[derive(thiserror::Error, Debug)]
pub enum PatriciaStorageError {
    /// An error that occurred in the database library.
    #[cfg(feature = "aerospike_storage")]
    #[error(transparent)]
    Aerospike(#[from] aerospike::Error),
    #[cfg(feature = "aerospike_storage")]
    #[error(transparent)]
    AerospikeStorage(#[from] crate::aerospike_storage::AerospikeStorageError),
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[cfg(any(test, feature = "mdbx_storage"))]
    #[error(transparent)]
    Mdbx(#[from] libmdbx::Error),
    #[cfg(any(test, feature = "rocksdb_storage"))]
    #[error(transparent)]
    Rocksdb(#[from] rust_rocksdb::Error),
    #[cfg(any(test, feature = "rocksdb_storage"))]
    #[error("Failed to fetch RocksDb stats.")]
    NoStats,
}

pub type PatriciaStorageResult<T> = Result<T, PatriciaStorageError>;

/// A trait for the statistics of a storage. Used as a trait bound for a storage associated stats
/// type.
pub trait StorageStats: Display {
    fn column_titles() -> Vec<&'static str>;

    fn column_values(&self) -> Vec<String>;

    fn stat_string(&self) -> String {
        Self::column_titles()
            .iter()
            .zip(self.column_values().iter())
            .map(|(title, value)| format!("{title}: {value}"))
            .collect::<Vec<String>>()
            .join(",")
    }
}

pub struct NoStats;

impl StorageStats for NoStats {
    fn column_titles() -> Vec<&'static str> {
        vec![]
    }

    fn column_values(&self) -> Vec<String> {
        vec![]
    }
}

impl Display for NoStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoStats")
    }
}

/// All configuration structs of storage implementations that can be used in apollo must implement
/// this trait.
pub trait StorageConfigTrait:
    Clone + Debug + Serialize + PartialEq + Validate + SerializeConfig + Default + Send + Sync
{
}
/// A trait for the storage. Does not assume concurrent access is possible - see [AsyncStorage].
pub trait Storage: Send + Sync {
    type Stats: StorageStats;
    type Config: StorageConfigTrait;

    /// Returns value from storage, if it exists.
    /// Uses a mutable &self to allow changes in the internal state of the storage (e.g.,
    /// for caching).
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn get(
        &mut self,
        key: &DbKey,
    ) -> impl Future<Output = PatriciaStorageResult<Option<DbValue>>> + Send;

    /// Sets value in storage. If key already exists, its value is overwritten.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn set(
        &mut self,
        key: DbKey,
        value: DbValue,
    ) -> impl Future<Output = PatriciaStorageResult<()>> + Send;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn mget(
        &mut self,
        keys: &[&DbKey],
    ) -> impl Future<Output = PatriciaStorageResult<Vec<Option<DbValue>>>> + Send;

    /// Sets values in storage.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn mset(
        &mut self,
        key_to_value: DbHashMap,
    ) -> impl Future<Output = PatriciaStorageResult<()>> + Send;

    /// Deletes a value from storage.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn delete(&mut self, key: &DbKey) -> impl Future<Output = PatriciaStorageResult<()>> + Send;

    /// If implemented, returns the statistics of the storage.
    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats>;

    /// If implemented, resets the statistics of the storage.
    fn reset_stats(&mut self) -> PatriciaStorageResult<()> {
        Ok(())
    }

    /// If the storage is async, returns an instance of the async storage.
    fn get_async_self(&self) -> Option<impl AsyncStorage>;
}

/// A trait wrapper for [Storage] that supports concurrency.
/// Any [Storage] implementation that implements `Clone` is an [AsyncStorage] as well.
pub trait AsyncStorage: Storage + Clone + 'static {}
impl<S: Storage + Clone + 'static> AsyncStorage for S {}

/// Empty config struct for storage implementations that don't require configuration.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct EmptyStorageConfig {}

impl Validate for EmptyStorageConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

impl SerializeConfig for EmptyStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}

impl StorageConfigTrait for EmptyStorageConfig {}

/// Dummy storage that does nothing.
#[derive(Clone)]
pub struct NullStorage;

impl Storage for NullStorage {
    type Stats = NoStats;
    type Config = EmptyStorageConfig;

    async fn get(&mut self, _key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(None)
    }

    async fn set(&mut self, _key: DbKey, _value: DbValue) -> PatriciaStorageResult<()> {
        Ok(())
    }

    async fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(vec![None; keys.len()])
    }

    async fn mset(&mut self, _key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        Ok(())
    }

    async fn delete(&mut self, _key: &DbKey) -> PatriciaStorageResult<()> {
        Ok(())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(NoStats)
    }

    fn get_async_self(&self) -> Option<impl AsyncStorage> {
        Some(self.clone())
    }
}

#[derive(Debug)]
pub struct DbKeyPrefix(Cow<'static, [u8]>);

impl DbKeyPrefix {
    pub const fn new(prefix: Cow<'static, [u8]>) -> Self {
        Self(prefix)
    }

    pub fn to_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<Felt> for DbKey {
    fn from(value: Felt) -> Self {
        DbKey(value.to_bytes_be().to_vec())
    }
}

/// To send storage to Python storage, it is necessary to serialize it.
impl Serialize for DbKey {
    /// Serializes `DbKey` to hexadecimal string representation.
    /// Needed since serde's Serialize derive attribute only works on
    /// HashMaps with String keys.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert Vec<u8> to hexadecimal string representation and serialize it.
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

/// Returns a `DbKey` from a prefix , separator, and suffix.
pub fn create_db_key(prefix: DbKeyPrefix, separator: &[u8], suffix: &[u8]) -> DbKey {
    DbKey([prefix.to_bytes(), separator, suffix].concat().to_vec())
}

/// Extracts the suffix from a `DbKey`. If the key doesn't match the prefix, None is returned.
pub fn try_extract_suffix_from_db_key<'a>(
    key: &'a DbKey,
    prefix: &DbKeyPrefix,
) -> Option<&'a [u8]> {
    // Ignore the ':' char that appears after the prefix.
    key.0.strip_prefix(prefix.to_bytes()).map(|s| &s[1..])
}
