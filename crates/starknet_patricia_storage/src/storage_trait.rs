use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;

use serde::{Serialize, Serializer};
use starknet_types_core::felt::Felt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DbKey(pub Vec<u8>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DbValue(pub Vec<u8>);

pub type DbHashMap = HashMap<DbKey, DbValue>;

/// An error that can occur when interacting with the database.
#[derive(thiserror::Error, Debug)]
pub enum PatriciaStorageError {
    /// An error that occurred in the database library.
    #[cfg(feature = "mdbx_storage")]
    #[error(transparent)]
    Mdbx(#[from] libmdbx::Error),
    #[cfg(feature = "rocksdb_storage")]
    #[error(transparent)]
    Rocksdb(#[from] rust_rocksdb::Error),
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

pub trait Storage {
    type Stats: StorageStats;

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
}

#[derive(Debug)]
pub struct DbKeyPrefix(&'static [u8]);

impl DbKeyPrefix {
    pub fn new(prefix: &'static [u8]) -> Self {
        Self(prefix)
    }

    pub fn to_bytes(&self) -> &'static [u8] {
        self.0
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

/// Returns a `DbKey` from a prefix and a suffix.
pub fn create_db_key(prefix: DbKeyPrefix, suffix: &[u8]) -> DbKey {
    DbKey([prefix.to_bytes().to_vec(), b":".to_vec(), suffix.to_vec()].concat())
}

/// Extracts the suffix from a `DbKey`. If the key doesn't match the prefix, None is returned.
pub fn try_extract_suffix_from_db_key<'a>(
    key: &'a DbKey,
    prefix: &DbKeyPrefix,
) -> Option<&'a [u8]> {
    // Ignore the ':' char that appears after the prefix.
    key.0.strip_prefix(prefix.to_bytes()).map(|s| &s[1..])
}
