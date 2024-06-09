use crate::patricia_merkle_tree::node_data::leaf::{ContractState, LeafDataImpl};
use crate::patricia_merkle_tree::types::SubTreeHeight;
use crate::storage::db_object::DBObject;
use crate::storage::storage_trait::{StoragePrefix, StorageValue};

impl DBObject for LeafDataImpl {
    /// Serializes the leaf data into a byte vector.
    /// The serialization is done as follows:
    /// - For storage values: serializes the value into a 32-byte vector.
    /// - For compiled class hashes or state tree tuples: creates a  json string
    ///   describing the leaf and cast it into a byte vector.
    fn serialize(&self) -> StorageValue {
        match &self {
            LeafDataImpl::StorageValue(value) => StorageValue(value.to_bytes_be().to_vec()),

            LeafDataImpl::CompiledClassHash(class_hash) => {
                let json_string =
                    format!(r#"{{"compiled_class_hash": "{}"}}"#, class_hash.0.to_hex());
                StorageValue(json_string.into_bytes())
            }

            LeafDataImpl::ContractState(ContractState {
                class_hash,
                storage_root_hash,
                nonce,
            }) => {
                let json_string = format!(
                    r#"{{"contract_hash": "{}", "storage_commitment_tree": {{"root": "{}", "height": {}}}, "nonce": "{}"}}"#,
                    class_hash.0.to_fixed_hex_string(),
                    storage_root_hash.0.to_fixed_hex_string(),
                    SubTreeHeight::ACTUAL_HEIGHT,
                    nonce.0.to_hex(),
                );
                StorageValue(json_string.into_bytes())
            }
        }
    }

    fn get_prefix(&self) -> StoragePrefix {
        match self {
            LeafDataImpl::StorageValue(_) => StoragePrefix::StorageLeaf,
            LeafDataImpl::CompiledClassHash(_) => StoragePrefix::CompiledClassLeaf,
            LeafDataImpl::ContractState { .. } => StoragePrefix::StateTreeLeaf,
        }
    }
}
