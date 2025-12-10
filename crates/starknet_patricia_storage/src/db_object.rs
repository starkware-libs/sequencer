use crate::errors::DeserializationError;
use crate::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};

pub type EmptyKeyContext = ();

pub trait HasDynamicPrefix {
    /// Extra data needed to construct a leaf for node db key prefix. For example, in index layout,
    /// we need to know the trie type of inner nodes.
    type KeyContext;

    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix;
}

pub trait HasStaticPrefix {
    /// Extra data needed to construct a leaf for node db key prefix. For example, in index layout,
    /// we need to know the trie type of inner nodes.
    type KeyContext;

    /// Returns the storage key prefix of the DB object.
    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix;
}

impl<T: HasStaticPrefix> HasDynamicPrefix for T {
    /// Inherit the KeyContext from the HasStaticPrefix trait.
    type KeyContext = T::KeyContext;

    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix {
        T::get_static_prefix(key_context)
    }
}

pub trait DBObject: HasDynamicPrefix {
    /// Serializes the given value.
    fn serialize(&self) -> DbValue;

    /// Returns a `DbKey` from a prefix and a suffix.
    fn get_db_key(&self, key_context: &Self::KeyContext, suffix: &[u8]) -> DbKey {
        create_db_key(self.get_prefix(key_context), suffix)
    }
}

pub trait Deserializable: Sized {
    /// Deserializes the given value.
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError>;
}
