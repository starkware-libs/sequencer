use crate::storage::errors::DeserializationError;
use crate::storage::storage_trait::{StorageKey, StorageValue};

pub trait DBObject {
    /// Serializes the given value.
    fn serialize(&self) -> StorageValue;

    // TODO(Aviv, 17/07/2024): Define a trait `T` for storage prefix and return `impl T` here.
    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self) -> Vec<u8>;

    /// Returns a `StorageKey` from a prefix and a suffix.
    fn get_db_key(&self, suffix: &[u8]) -> StorageKey {
        StorageKey([self.get_prefix(), b":".to_vec(), suffix.to_vec()].concat())
    }
}

pub trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError>;

    // TODO(Aviv, 17/07/2024): Define a trait `T` for storage prefix and return `impl T` here.
    /// The prefix used to store in DB.
    fn prefix() -> Vec<u8>;
}
