use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use crate::patricia_merkle_tree::node_data::leaf::ContractState;
use crate::patricia_merkle_tree::types::SubTreeHeight;
use crate::storage::db_object::DBObject;
use crate::storage::storage_trait::{StoragePrefix, StorageValue};

impl DBObject for StarknetStorageValue {
    /// Serializes the value into a 32-byte vector.
    fn serialize(&self) -> StorageValue {
        StorageValue(self.0.to_bytes_be().to_vec())
    }

    fn get_prefix(&self) -> StoragePrefix {
        StoragePrefix::StorageLeaf
    }
}

impl DBObject for CompiledClassHash {
    /// Creates a json string describing the leaf and casts it into a byte vector.
    fn serialize(&self) -> StorageValue {
        let json_string = format!(r#"{{"compiled_class_hash": "{}"}}"#, self.0.to_hex());
        StorageValue(json_string.into_bytes())
    }

    fn get_prefix(&self) -> StoragePrefix {
        StoragePrefix::CompiledClassLeaf
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

    fn get_prefix(&self) -> StoragePrefix {
        StoragePrefix::StateTreeLeaf
    }
}
