use crate::errors::DeserializationError;
use crate::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};

pub trait HasDynamicPrefix {
    type KeyContext;

    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix;
}

pub trait HasStaticPrefix {
    type KeyContext;

    /// Returns the storage key prefix of the DB object.
    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix;
}

impl<T: HasStaticPrefix> HasDynamicPrefix for T {
    type KeyContext = <T as HasStaticPrefix>::KeyContext;
    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix {
        T::get_static_prefix(key_context)
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
