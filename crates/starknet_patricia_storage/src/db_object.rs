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

pub trait DBObject: Sized + HasDynamicPrefix {
    type DeserializeContext;

    fn serialize(&self) -> DbValue;
    fn deserialize(
        value: &DbValue,
        deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError>;

    fn get_db_key(&self, key_context: &Self::KeyContext, suffix: &[u8]) -> DbKey {
        create_db_key(self.get_prefix(key_context), suffix)
    }
}
