use crate::errors::DeserializationError;
use crate::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};

pub struct EmptyKeyContext;

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

pub struct EmptyDeserializationContext;

pub trait DBObject: Sized + HasDynamicPrefix {
    /// Extra data needed to deserialize the object. For example, facts layout nodes need the node
    /// hash and an indication of whether or not it's a leaf node (index layout nodes only need the
    /// latter).
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
