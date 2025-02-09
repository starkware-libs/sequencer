use crate::errors::DeserializationError;
use crate::storage_trait::{create_db_key, DbKey, DbValue, StoragePrefix};

pub trait HasDynamicPrefix {
    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self) -> StoragePrefix;
}

pub trait HasStaticPrefix {
    /// Returns the storage key prefix of the DB object.
    fn get_static_prefix() -> StoragePrefix;
}

impl<T: HasStaticPrefix> HasDynamicPrefix for T {
    fn get_prefix(&self) -> StoragePrefix {
        T::get_static_prefix()
    }
}

pub trait DBObject: HasDynamicPrefix {
    /// Serializes the given value.
    fn serialize(&self) -> DbValue;

    /// Returns a `DbKey` from a prefix and a suffix.
    fn get_db_key(&self, suffix: &[u8]) -> DbKey {
        create_db_key(self.get_prefix(), suffix)
    }
}

pub trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError>;
}
