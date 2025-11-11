use std::collections::HashMap;
use std::fmt::Display;

use serde::{Serialize, Serializer};
use starknet_api::core::ascii_as_felt;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DbKey(pub Vec<u8>);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DbValue(pub Vec<u8>);

pub type DbHashMap = HashMap<TrieKey, DbValue>;

// TODO: further refactor to node index + prefix instead of DbKey.
#[derive(Clone, Debug, Serialize, Eq, PartialEq, Hash)]
pub enum TrieKey {
    LatestTrie(DbKey),
    HistoricalTries(DbKey, BlockNumber),
}

impl TrieKey {
    // TODO: take NodeIndex instead of raw bytes, separate from starknet_patricia.
    pub fn from_node_index_and_context(node_index_bytes: Vec<u8>, context: &KeyContext) -> Self {
        let prefix = context.trie_type.get_prefix();
        let key = prefix.into_iter().chain(node_index_bytes).collect::<Vec<u8>>();
        match &context.block_number {
            Some(block_number) => TrieKey::HistoricalTries(DbKey(key), *block_number),
            None => TrieKey::LatestTrie(DbKey(key)),
        }
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct KeyContext {
    pub trie_type: TrieType,
    pub block_number: Option<BlockNumber>,
}

#[derive(Debug, PartialEq, Default)]
pub enum TrieType {
    ContractsTrie,
    ClassesTrie,
    StorageTrie(Felt),
    #[default]
    GeneralTrie,
}

impl TrieType {
    pub fn get_prefix(&self) -> Vec<u8> {
        match self {
            TrieType::ContractsTrie => {
                ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec()
            }
            TrieType::ClassesTrie => {
                ascii_as_felt("CLASSES_TREE_PREFIX").unwrap().to_bytes_be().to_vec()
            }
            TrieType::StorageTrie(contract_address) => (*contract_address).to_bytes_be().to_vec(),
            TrieType::GeneralTrie => vec![],
        }
    }
}

impl From<DbKey> for TrieKey {
    fn from(key: DbKey) -> Self {
        TrieKey::LatestTrie(key)
    }
}

impl From<TrieKey> for DbKey {
    fn from(key: TrieKey) -> Self {
        match key {
            TrieKey::LatestTrie(key) => key,
            TrieKey::HistoricalTries(key, _) => key,
        }
    }
}

impl<'a> From<&'a TrieKey> for &'a DbKey {
    fn from(key: &'a TrieKey) -> Self {
        match key {
            TrieKey::LatestTrie(key) => key,
            TrieKey::HistoricalTries(key, _) => key,
        }
    }
}
// impl Into<DbKey> for TrieKey {
//     fn into(self) -> DbKey {
//         match self {
//             TrieKey::LatestTrie(key) => key,
//             TrieKey::HistoricalTries(key, _) => key,
//         }
//     }
// }

// impl<'a> Into<&'a DbKey> for &'a TrieKey {
//     fn into(self) -> &'a DbKey {
//         match self {
//             TrieKey::LatestTrie(key) => key,
//             TrieKey::HistoricalTries(key, _) => key,
//         }
//     }
// }

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq, Hash)]
pub struct BlockNumber(pub u64);

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
    #[error("Multiple timestamps are not supported")]
    MultipleTimestamps,
    #[error("Deletion from historical ties is not are not supported")]
    AttemptToModifyHistory,
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
    fn get(&mut self, key: &TrieKey) -> PatriciaStorageResult<Option<DbValue>>;

    /// Sets value in storage. If key already exists, its value is overwritten and the old value is
    /// returned.
    fn set(&mut self, key: TrieKey, value: DbValue) -> PatriciaStorageResult<()>;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    fn mget(&mut self, keys: &[&TrieKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>>;

    /// Sets values in storage.
    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()>;

    /// Deletes value from storage and returns its value if it exists. Returns None if not.
    fn delete(&mut self, key: &TrieKey) -> PatriciaStorageResult<()>;

    /// If implemented, returns the statistics of the storage.
    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats>;
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
