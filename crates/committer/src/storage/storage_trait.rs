use crate::storage::errors::StorageError;
use std::collections::HashMap;

#[allow(dead_code)]
pub(crate) struct StorageKey(Vec<u8>);

#[allow(dead_code)]
pub(crate) struct StorageValue(Vec<u8>);

pub(crate) trait Storage {
    /// Returns value from storage, if it exists.
    fn get(&self, key: &StorageKey) -> Option<StorageValue>;
    /// Sets value in storage.
    fn set(&mut self, key: &StorageKey, value: &StorageValue);
    /// Returns values from storage in same order of given keys. If key does not exist,
    /// value is None.
    fn mget(&self, keys: &[StorageKey]) -> [Option<StorageValue>];
    /// Sets values in storage.
    fn mset(&mut self, key_to_value: &HashMap<StorageKey, StorageValue>);
    /// Deletes value from storage. Returns error if key does not exist.
    fn delete(&mut self, key: &StorageKey) -> Result<(), StorageError>;
}
