use crate::storage::errors::SerializationError;
use crate::storage::storage_trait::{StorageKey, StorageValue};

pub(crate) trait Serializable {
    /// Serializes the given value.
    fn serialize(&self) -> Result<StorageValue, SerializationError>;
    /// Deserializes the given value.
    fn deserialize(key: StorageKey, value: StorageValue) -> Result<Self, SerializationError>
    where
        Self: Sized;
    /// Returns the key used to store self in storage.
    fn db_key(&self) -> StorageKey;
}
