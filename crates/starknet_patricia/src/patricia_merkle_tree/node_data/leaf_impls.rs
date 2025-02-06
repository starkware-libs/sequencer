use std::collections::HashMap;

use starknet_api::core::CompiledClassHash;
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::node_data::errors::LeafResult;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::storage::db_object::{DBObject, Deserializable};
use crate::storage::errors::DeserializationError;
use crate::storage::storage_trait::{StarknetPrefix, StorageValue};

impl Deserializable for CompiledClassHash {
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError> {
        let json_str = std::str::from_utf8(&value.0)?;
        let map: HashMap<String, String> = serde_json::from_str(json_str)?;
        let hash_as_hex = map
            .get("compiled_class_hash")
            .ok_or(DeserializationError::NonExistingKey("compiled_class_hash".to_string()))?;
        Ok(Self(Felt::from_hex(hash_as_hex)?))
    }

    fn prefix() -> Vec<u8> {
        StarknetPrefix::CompiledClassLeaf.to_storage_prefix()
    }
}

impl DBObject for CompiledClassHash {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> StorageValue {
        let json_string = format!(r#"{{"compiled_class_hash": "{}"}}"#, self.0.to_hex_string());
        StorageValue(json_string.into_bytes())
    }

    fn get_prefix(&self) -> Vec<u8> {
        StarknetPrefix::CompiledClassLeaf.to_storage_prefix()
    }
}

impl Leaf for CompiledClassHash {
    type Input = Self;
    type Output = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        Ok((input, ()))
    }
}
