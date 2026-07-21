use crate::errors::{DeserializationError, SerializationResult};
use crate::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};

pub struct EmptyKeyContext;

pub trait HasDynamicPrefix {
    /// Extra data needed to construct a leaf for node db key prefix. For example, in index layout,
    /// we need to know the trie type of inner nodes.
    type KeyContext;

    /// Returns the storage key prefix of the DB object.
    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix;

    /// Returns the storage key prefix without an instance, when it depends only on
    /// `key_context` and not on the object's data (e.g. node type). Callers that compute the
    /// prefix for many objects sharing the same `key_context` (such as all the nodes of a single
    /// contract's storage trie) can call this once and reuse the result, instead of paying the
    /// prefix's allocation cost per object. Returns `None` when the prefix is data-dependent, in
    /// which case callers must fall back to per-object `get_prefix`.
    fn static_prefix_hint(_key_context: &Self::KeyContext) -> Option<DbKeyPrefix> {
        None
    }
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

    fn static_prefix_hint(key_context: &Self::KeyContext) -> Option<DbKeyPrefix> {
        Some(T::get_static_prefix(key_context))
    }
}

pub struct EmptyDeserializationContext;

pub trait DBObject: Sized + HasDynamicPrefix {
    /// The separator between the prefix and the suffix in the db key.
    const DB_KEY_SEPARATOR: &[u8];

    /// Extra data needed to deserialize the object. For example, facts layout nodes need the node
    /// hash and an indication of whether or not it's a leaf node (index layout nodes only need the
    /// latter).
    type DeserializeContext;

    /// Serializes the given value.
    fn serialize(&self) -> SerializationResult<DbValue>;

    /// Deserializes the given value using the provided context.
    fn deserialize(
        value: &DbValue,
        deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError>;

    /// Returns a [DbKey] from a prefix and a suffix.
    fn get_db_key(&self, key_context: &Self::KeyContext, suffix: &[u8]) -> DbKey {
        create_db_key(&self.get_prefix(key_context), Self::DB_KEY_SEPARATOR, suffix)
    }

    /// Returns a [DbKey] from a precomputed prefix (see [HasDynamicPrefix::static_prefix_hint])
    /// and a suffix, without needing an instance.
    fn db_key_from_prefix(prefix: &DbKeyPrefix, suffix: &[u8]) -> DbKey {
        create_db_key(prefix, Self::DB_KEY_SEPARATOR, suffix)
    }
}
