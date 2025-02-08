use crate::errors::DeserializationError;
use crate::storage_trait::{create_db_key, DbStorageKey, DbStorageValue, StoragePrefix};

pub trait DBObject {
    /// Serializes the given value.
    fn serialize(&self) -> DbStorageValue;

    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self) -> StoragePrefix;

    /// Returns a `DbStorageKey` from a prefix and a suffix.
    fn get_db_key(&self, suffix: &[u8]) -> DbStorageKey {
        create_db_key(self.get_prefix(), suffix)
    }
}

pub trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(value: &DbStorageValue) -> Result<Self, DeserializationError>;
}
