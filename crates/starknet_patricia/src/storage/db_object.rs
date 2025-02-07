use crate::storage::errors::DeserializationError;
use crate::storage::storage_trait::{create_db_key, StorageKey, StoragePrefix, StorageValue};

pub trait DBObject {
    /// Serializes the given value.
    fn serialize(&self) -> StorageValue;

    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self) -> impl StoragePrefix;

    /// Returns a `StorageKey` from a prefix and a suffix.
    fn get_db_key(&self, suffix: &[u8]) -> StorageKey {
        create_db_key(self.get_prefix(), suffix)
    }
}

pub trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError>;
}
