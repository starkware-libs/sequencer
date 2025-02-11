use std::collections::HashMap;

use serde::{Serialize, Serializer};
use starknet_types_core::felt::Felt;

#[derive(Debug, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct DbKey(pub Vec<u8>);

#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
pub struct DbValue(pub Vec<u8>);

pub trait Storage: From<HashMap<DbKey, DbValue>> {
    /// Returns value from storage, if it exists.
    fn get(&self, key: &DbKey) -> Option<&DbValue>;

    /// Sets value in storage. If key already exists, its value is overwritten and the old value is
    /// returned.
    fn set(&mut self, key: DbKey, value: DbValue) -> Option<DbValue>;

    /// Returns values from storage in same order of given keys. Value is None for keys that do not
    /// exist.
    fn mget(&self, keys: &[DbKey]) -> Vec<Option<&DbValue>>;

    /// Sets values in storage.
    fn mset(&mut self, key_to_value: HashMap<DbKey, DbValue>);

    /// Deletes value from storage and returns its value if it exists. Returns None if not.
    fn delete(&mut self, key: &DbKey) -> Option<DbValue>;
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
