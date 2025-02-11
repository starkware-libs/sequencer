use std::collections::HashMap;

use serde::{Serialize, Serializer};
use starknet_types_core::felt::Felt;

#[derive(Debug, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct StorageKey(pub Vec<u8>);

#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct StorageValue(pub Vec<u8>);

pub trait Storage: From<HashMap<StorageKey, StorageValue>> {
    /// Returns value from storage, if it exists.
    fn get(&self, key: &StorageKey) -> Option<&StorageValue>;

    /// Sets value in storage. If key already exists, its value is overwritten and the old value is
    /// returned.
    fn set(&mut self, key: StorageKey, value: StorageValue) -> Option<StorageValue>;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    fn mget(&self, keys: &[StorageKey]) -> Vec<Option<&StorageValue>>;

    /// Sets values in storage.
    fn mset(&mut self, key_to_value: HashMap<StorageKey, StorageValue>);

    /// Deletes value from storage and returns its value if it exists. Returns None if not.
    fn delete(&mut self, key: &StorageKey) -> Option<StorageValue>;
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

impl From<Felt> for StorageKey {
    fn from(value: Felt) -> Self {
        StorageKey(value.to_bytes_be().to_vec())
    }
}

/// To send storage to Python storage, it is necessary to serialize it.
impl Serialize for StorageKey {
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

/// Returns a `StorageKey` from a prefix and a suffix.
pub fn create_db_key(prefix: StoragePrefix, suffix: &[u8]) -> StorageKey {
    StorageKey([prefix.to_bytes().to_vec(), b":".to_vec(), suffix.to_vec()].concat())
}
