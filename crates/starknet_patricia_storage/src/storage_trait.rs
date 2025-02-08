use std::collections::HashMap;

use serde::{Serialize, Serializer};
use starknet_types_core::felt::Felt;

#[derive(Debug, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct DbStorageKey(pub Vec<u8>);

#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct DbStorageValue(pub Vec<u8>);

pub trait Storage: From<HashMap<DbStorageKey, DbStorageValue>> {
    /// Returns value from storage, if it exists.
    fn get(&self, key: &DbStorageKey) -> Option<&DbStorageValue>;

    /// Sets value in storage. If key already exists, its value is overwritten and the old value is
    /// returned.
    fn set(&mut self, key: DbStorageKey, value: DbStorageValue) -> Option<DbStorageValue>;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    fn mget(&self, keys: &[DbStorageKey]) -> Vec<Option<&DbStorageValue>>;

    /// Sets values in storage.
    fn mset(&mut self, key_to_value: HashMap<DbStorageKey, DbStorageValue>);

    /// Deletes value from storage and returns its value if it exists. Returns None if not.
    fn delete(&mut self, key: &DbStorageKey) -> Option<DbStorageValue>;
}

#[derive(Debug)]
pub struct StoragePrefix(&'static [u8]);

impl StoragePrefix {
    pub fn new(prefix: &'static [u8]) -> Self {
        Self(prefix)
    }

    pub fn to_bytes(&self) -> &'static [u8] {
        self.0
    }
}

impl From<Felt> for DbStorageKey {
    fn from(value: Felt) -> Self {
        DbStorageKey(value.to_bytes_be().to_vec())
    }
}

/// To send storage to Python storage, it is necessary to serialize it.
impl Serialize for DbStorageKey {
    /// Serializes `StorageKey` to hexadecimal string representation.
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

/// Returns a `DbStorageKey` from a prefix and a suffix.
pub fn create_db_key(prefix: StoragePrefix, suffix: &[u8]) -> DbStorageKey {
    DbStorageKey([prefix.to_bytes().to_vec(), b":".to_vec(), suffix.to_vec()].concat())
}
