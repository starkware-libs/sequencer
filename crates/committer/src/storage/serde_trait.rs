use crate::storage::errors::SerdeError;
use crate::storage::storage_trait::{StorageError, StorageKey, StorageValue};

pub(crate) trait Serializable {
    /// Serializes the given value.
    fn serialize(&self) -> Result<StorageValue, SerializationError::Serialize>;
    /// Deserializes the given value.
    fn deserialize(value: StorageValue) -> Result<Self, SerializationError::Deserialize>;
    /// Returns the key used to store self in storage.
    fn db_key(&self) -> StorageKey;
}
