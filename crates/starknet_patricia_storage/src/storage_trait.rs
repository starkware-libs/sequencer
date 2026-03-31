use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Debug, Display};
use std::future::Future;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{ParamPath, SerializedParam};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize, Serializer};
use starknet_types_core::felt::Felt;
use tokio::task::JoinError;
use validator::Validate;

use crate::errors::DeserializationError;
use crate::reads_collector_storage::ReadsCollectorStorage;

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

#[derive(Clone, Eq, PartialEq, Serialize)]
pub enum DbOperation {
    Set(DbValue),
    Delete,
}

pub type DbOperationMap = HashMap<DbKey, DbOperation>;

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
    #[error(transparent)]
    Join(#[from] JoinError),
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

/// A read-only view of a storage that does not require mutable access.
/// Allows concurrent reads from multiple threads.
pub trait ImmutableReadOnlyStorage: Send + Sync + 'static {
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn get(
        &self,
        key: &DbKey,
    ) -> impl Future<Output = PatriciaStorageResult<Option<DbValue>>> + Send;

    fn mget(
        &self,
        keys: &[&DbKey],
    ) -> impl Future<Output = PatriciaStorageResult<Vec<Option<DbValue>>>> + Send;

    /// Runs the given tasks concurrently, each with its own [ReadsCollectorStorage] snapshot.
    /// By default, discards collected reads.
    fn gather<T>(&mut self, tasks: Vec<T>) -> impl Future<Output = Vec<T::Output>> + Send
    where
        T: for<'s> StorageTask<'s, Self> + Send,
        Self: Sized,
    {
        async move {
            let (_reads, outputs) = run_tasks_and_collect_reads(self, tasks).await;
            outputs
        }
    }
}

/// Defines the output type of a [StorageTask], split out so `gather` can reference `T::Output`
/// without requiring a lifetime parameter.
pub trait StorageTaskOutput<S: ImmutableReadOnlyStorage>: Send {
    type Output: Send;
}

/// A unit of work that reads from storage concurrently.
/// Implementors hold all data needed to perform the read and produce an output.
#[async_trait]
pub trait StorageTask<'a, S: ImmutableReadOnlyStorage + 'a>: StorageTaskOutput<S> + Send {
    async fn run_with_storage(self, storage: &mut ReadsCollectorStorage<'a, S>) -> Self::Output;
}

/// Runs tasks concurrently, each with its own [ReadsCollectorStorage] snapshot.
/// Returns the merged reads from all tasks alongside the outputs.
pub(crate) async fn run_tasks_and_collect_reads<'a, S, T>(
    storage: &'a S,
    tasks: Vec<T>,
) -> (DbHashMap, Vec<T::Output>)
where
    S: ImmutableReadOnlyStorage,
    T: StorageTask<'a, S> + Send,
{
    let futures = tasks.into_iter().map(|task| async move {
        let mut collector = ReadsCollectorStorage::new(storage);
        let output = task.run_with_storage(&mut collector).await;
        (collector.into_reads(), output)
    });
    let mut collected_reads = DbHashMap::new();
    let (reads_vec, outputs): (Vec<_>, Vec<_>) = join_all(futures).await.into_iter().unzip();
    for reads in reads_vec {
        collected_reads.extend(reads);
    }
    (collected_reads, outputs)
}

/// A read-only view of a storage. Does not assume concurrent access is possible.
pub trait ReadOnlyStorage: Send + Sync {
    /// Returns value from storage, if it exists.
    /// Uses a mutable `&self` to allow changes in the internal state of the storage (e.g.,
    /// for caching).
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn get_mut(
        &mut self,
        key: &DbKey,
    ) -> impl Future<Output = PatriciaStorageResult<Option<DbValue>>> + Send;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn mget_mut(
        &mut self,
        keys: &[&DbKey],
    ) -> impl Future<Output = PatriciaStorageResult<Vec<Option<DbValue>>>> + Send;
}

/// A trait for the storage. Extends [ReadOnlyStorage] with write operations.
pub trait Storage: ReadOnlyStorage {
    type Stats: StorageStats;
    type Config: StorageConfigTrait;

    /// Sets value in storage. If key already exists, its value is overwritten.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn set(
        &mut self,
        key: DbKey,
        value: DbValue,
    ) -> impl Future<Output = PatriciaStorageResult<()>> + Send;

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

    /// Sets values in storage and deletes keys from storage in a single operation.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn multi_set_and_delete(
        &mut self,
        key_to_operation: DbOperationMap,
    ) -> impl Future<Output = PatriciaStorageResult<()>> + Send;

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

impl ImmutableReadOnlyStorage for NullStorage {
    async fn get(&self, _key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        Ok(None)
    }

    async fn mget(&self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        Ok(vec![None; keys.len()])
    }
}

impl ReadOnlyStorage for NullStorage {
    async fn get_mut(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        ImmutableReadOnlyStorage::get(self, key).await
    }

    async fn mget_mut(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        ImmutableReadOnlyStorage::mget(self, keys).await
    }
}

impl Storage for NullStorage {
    type Stats = NoStats;
    type Config = EmptyStorageConfig;

    async fn set(&mut self, _key: DbKey, _value: DbValue) -> PatriciaStorageResult<()> {
        Ok(())
    }

    async fn mset(&mut self, _key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        Ok(())
    }

    async fn delete(&mut self, _key: &DbKey) -> PatriciaStorageResult<()> {
        Ok(())
    }

    async fn multi_set_and_delete(
        &mut self,
        _key_to_operation: DbOperationMap,
    ) -> PatriciaStorageResult<()> {
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
