use std::borrow::Cow;

use super::{DBObject, EmptyDeserializationContext, HasDynamicPrefix, HasStaticPrefix};
use crate::errors::{DeserializationError, SerializationResult};
use crate::storage_trait::{DbKeyPrefix, DbValue};

/// A key context whose bytes participate in the prefix, mirroring index layout's use of the
/// contract address as the storage-trie prefix. Guards that the hoisted prefix is derived from
/// `key_context` and nothing else.
struct PrefixKeyContext(Vec<u8>);

/// A minimal `DBObject` that carries data unrelated to its prefix, so we can assert the prefix is
/// independent of the object's contents.
struct StaticPrefixObject {
    unrelated_data: u8,
}

impl HasStaticPrefix for StaticPrefixObject {
    type KeyContext = PrefixKeyContext;

    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix {
        DbKeyPrefix::new(Cow::Owned(key_context.0.clone()))
    }
}

impl DBObject for StaticPrefixObject {
    const DB_KEY_SEPARATOR: &[u8] = b":";

    type DeserializeContext = EmptyDeserializationContext;

    fn serialize(&self) -> SerializationResult<DbValue> {
        Ok(DbValue(vec![self.unrelated_data]))
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(Self { unrelated_data: value.0[0] })
    }
}

#[test]
fn static_prefix_hint_matches_get_prefix_for_static_prefix_types() {
    let key_context = PrefixKeyContext(vec![7, 8, 9]);
    let object = StaticPrefixObject { unrelated_data: 42 };

    let hint = StaticPrefixObject::static_prefix_hint(&key_context);
    assert_eq!(
        hint.as_ref().map(DbKeyPrefix::to_bytes),
        Some(object.get_prefix(&key_context).to_bytes()),
        "the hoisted hint must equal the per-instance prefix",
    );
}

#[test]
fn hoisted_and_per_instance_db_keys_are_identical() {
    let key_context = PrefixKeyContext(vec![7, 8, 9]);
    let suffix = [1, 2, 3, 4];

    // Two objects with different data but the same key context must produce the same key via the
    // hoisted path, and it must match the per-instance path.
    let first_object = StaticPrefixObject { unrelated_data: 1 };
    let second_object = StaticPrefixObject { unrelated_data: 2 };

    let prefix = StaticPrefixObject::static_prefix_hint(&key_context)
        .expect("a static-prefix type must yield a hint");
    let hoisted_key = StaticPrefixObject::db_key_from_prefix(&prefix, &suffix);

    assert_eq!(hoisted_key, first_object.get_db_key(&key_context, &suffix));
    assert_eq!(hoisted_key, second_object.get_db_key(&key_context, &suffix));
}
