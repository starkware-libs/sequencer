use crate::storage::errors::{DeserializationError, SerializationError};
use crate::storage::storage_trait::{StorageKey, StorageValue};

pub(crate) trait Serializable {
    /// Serializes the given value.
    fn serialize(&self) -> Result<StorageValue, SerializationError>;
    /// Returns the key used to store self in storage.
    fn db_key(&self) -> StorageKey;
}

pub(crate) trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(key: &StorageKey, value: &StorageValue) -> Result<Self, DeserializationError>
    where
        Self: Sized;
}
