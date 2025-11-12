use std::collections::HashMap;

use serde_json::Value;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_patricia_storage::db_object::{DBObject, Deserializable};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{fixed_hex_string_no_prefix, CompiledClassHash};

#[derive(Clone, Debug)]
pub enum CommitterLeafPrefix {
    StorageLeaf,
    StateTreeLeaf,
    CompiledClassLeaf,
}

impl From<CommitterLeafPrefix> for DbKeyPrefix {
    fn from(value: CommitterLeafPrefix) -> Self {
        match value {
            CommitterLeafPrefix::StorageLeaf => Self::new(b"starknet_storage_leaf"),
            CommitterLeafPrefix::StateTreeLeaf => Self::new(b"contract_state"),
            CommitterLeafPrefix::CompiledClassLeaf => Self::new(b"contract_class_leaf"),
        }
    }
}

impl DBObject for StarknetStorageValue {
    /// Serializes the value into a 32-byte vector.
    fn serialize(&self) -> DbValue {
        DbValue(self.0.to_bytes_be().to_vec())
    }
}

impl DBObject for CompiledClassHash {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> DbValue {
        let json_string = format!(r#"{{"compiled_class_hash": "{}"}}"#, self.0.to_hex_string());
        DbValue(json_string.into_bytes())
    }
}

impl DBObject for ContractState {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> DbValue {
        let json_string = format!(
            r#"{{"contract_hash": "{}", "storage_commitment_tree": {{"root": "{}", "height": {}}}, "nonce": "{}"}}"#,
            fixed_hex_string_no_prefix(&self.class_hash.0),
            fixed_hex_string_no_prefix(&self.storage_root_hash.0),
            SubTreeHeight::ACTUAL_HEIGHT,
            self.nonce.0.to_hex_string(),
        );
        DbValue(json_string.into_bytes())
    }
}

impl Deserializable for StarknetStorageValue {
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        Ok(Self(Felt::from_bytes_be_slice(&value.0)))
    }
}

impl Deserializable for CompiledClassHash {
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        let json_str = std::str::from_utf8(&value.0)?;
        let map: HashMap<String, String> = serde_json::from_str(json_str)?;
        let hash_as_hex = map
            .get("compiled_class_hash")
            .ok_or(DeserializationError::NonExistingKey("compiled_class_hash".to_string()))?;
        Ok(Self::from_hex(hash_as_hex)?)
    }
}

impl Deserializable for ContractState {
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        let json_str = std::str::from_utf8(&value.0)?;
        let deserialized_map: Value = serde_json::from_str(json_str)?;
        let get_leaf_key = |map: &Value, key: &str| {
            let s = get_key_from_map(map, key)?
                .as_str()
                .ok_or(DeserializationError::LeafTypeError)?
                .to_string();
            Ok::<String, DeserializationError>(s)
        };
        let class_hash_as_hex = get_leaf_key(&deserialized_map, "contract_hash")?;
        // Contracts that were created before the Starknet protocol supported the nonce field might
        // not include it.
        let nonce_as_hex = get_leaf_key(&deserialized_map, "nonce").unwrap_or("0x0".to_string());
        let root_hash_as_hex =
            get_leaf_key(get_key_from_map(&deserialized_map, "storage_commitment_tree")?, "root")?;

        Ok(Self {
            nonce: Nonce(Felt::from_hex(&nonce_as_hex)?),
            storage_root_hash: HashOutput::from_hex(&root_hash_as_hex)?,
            class_hash: ClassHash(Felt::from_hex(&class_hash_as_hex)?),
        })
    }
}

fn get_key_from_map<'a>(map: &'a Value, key: &str) -> Result<&'a Value, DeserializationError> {
    map.get(key).ok_or(DeserializationError::NonExistingKey(key.to_string()))
}
