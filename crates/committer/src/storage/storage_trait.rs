use crate::types::Felt;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Eq, Hash, PartialEq)]
pub(crate) struct StorageKey(pub(crate) Vec<u8>);

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageValue(pub(crate) Vec<u8>);

pub(crate) trait Storage {
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

pub(crate) enum StoragePrefix {
    PatriciaNode,
}

impl StoragePrefix {
    pub(crate) fn to_bytes(&self) -> &[u8] {
        match self {
            Self::PatriciaNode => "patricia_node:".as_bytes(),
        }
    }
}

impl StorageKey {
    pub(crate) fn with_prefix(&self, prefix: StoragePrefix) -> Self {
        let mut prefix = prefix.to_bytes().to_vec();
        prefix.extend(&self.0);
        StorageKey(prefix)
    }
}

impl From<Felt> for StorageKey {
    fn from(value: Felt) -> Self {
        StorageKey(value.to_bytes_be().to_vec())
    }
}
