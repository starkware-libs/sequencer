use std::collections::HashMap;

use starknet_api::core::CompiledClassHash;
use starknet_types_core::felt::Felt;

use crate::db_object::{DBObject, Deserializable, HasStaticPrefix};
use crate::errors::DeserializationError;
use crate::storage_trait::{DbKeyPrefix, DbValue};

impl HasStaticPrefix for CompiledClassHash {
    fn get_static_prefix() -> DbKeyPrefix {
        DbKeyPrefix::new(b"contract_class_leaf")
    }
}

impl Deserializable for CompiledClassHash {
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        let json_str = std::str::from_utf8(&value.0)?;
        let map: HashMap<String, String> = serde_json::from_str(json_str)?;
        let hash_as_hex = map
            .get("compiled_class_hash")
            .ok_or(DeserializationError::NonExistingKey("compiled_class_hash".to_string()))?;
        Ok(Self(Felt::from_hex(hash_as_hex)?))
    }
}

impl DBObject for CompiledClassHash {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> DbValue {
        let json_string = format!(r#"{{"compiled_class_hash": "{}"}}"#, self.0.to_hex_string());
        DbValue(json_string.into_bytes())
    }
}
