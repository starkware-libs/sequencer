use std::collections::HashMap;

use serde_json::Value;
use starknet_patricia::felt::Felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_patricia::storage::db_object::{DBObject, Deserializable};
use starknet_patricia::storage::errors::DeserializationError;
use starknet_patricia::storage::storage_trait::{StarknetPrefix, StorageValue};

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{ClassHash, CompiledClassHash, Nonce};

impl DBObject for StarknetStorageValue {
    /// Serializes the value into a 32-byte vector.
    fn serialize(&self) -> StorageValue {
        StorageValue(self.0.to_bytes_be().to_vec())
    }

    fn get_prefix(&self) -> Vec<u8> {
        StarknetPrefix::StorageLeaf.to_storage_prefix()
    }
}

impl DBObject for CompiledClassHash {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> StorageValue {
        let json_string = format!(r#"{{"compiled_class_hash": "{}"}}"#, self.0.to_hex());
        StorageValue(json_string.into_bytes())
    }

    fn get_prefix(&self) -> Vec<u8> {
        StarknetPrefix::CompiledClassLeaf.to_storage_prefix()
    }
}

impl DBObject for ContractState {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> StorageValue {
        let json_string = format!(
            r#"{{"contract_hash": "{}", "storage_commitment_tree": {{"root": "{}", "height": {}}}, "nonce": "{}"}}"#,
            self.class_hash.0.to_fixed_hex_string(),
            self.storage_root_hash.0.to_fixed_hex_string(),
            SubTreeHeight::ACTUAL_HEIGHT,
            self.nonce.0.to_hex(),
        );
        StorageValue(json_string.into_bytes())
    }

    fn get_prefix(&self) -> Vec<u8> {
        StarknetPrefix::StateTreeLeaf.to_storage_prefix()
    }
}

impl Deserializable for StarknetStorageValue {
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError> {
        Ok(Self(Felt::from_bytes_be_slice(&value.0)))
    }

    fn prefix() -> Vec<u8> {
        StarknetPrefix::StorageLeaf.to_storage_prefix()
    }
}

impl Deserializable for CompiledClassHash {
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError> {
        let json_str = std::str::from_utf8(&value.0)?;
        let map: HashMap<String, String> = serde_json::from_str(json_str)?;
        let hash_as_hex = map
            .get("compiled_class_hash")
            .ok_or(DeserializationError::NonExistingKey("compiled_class_hash".to_string()))?;
        Ok(Self::from_hex(hash_as_hex)?)
    }

    fn prefix() -> Vec<u8> {
        StarknetPrefix::CompiledClassLeaf.to_storage_prefix()
    }
}

impl Deserializable for ContractState {
    fn deserialize(value: &StorageValue) -> Result<Self, DeserializationError> {
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
            nonce: Nonce::from_hex(&nonce_as_hex)?,
            storage_root_hash: HashOutput::from_hex(&root_hash_as_hex)?,
            class_hash: ClassHash::from_hex(&class_hash_as_hex)?,
        })
    }

    fn prefix() -> Vec<u8> {
        StarknetPrefix::StateTreeLeaf.to_storage_prefix()
    }
}

fn get_key_from_map<'a>(map: &'a Value, key: &str) -> Result<&'a Value, DeserializationError> {
    map.get(key).ok_or(DeserializationError::NonExistingKey(key.to_string()))
}
