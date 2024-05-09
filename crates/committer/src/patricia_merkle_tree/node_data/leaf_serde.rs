use crate::patricia_merkle_tree::node_data::leaf::{ContractState, LeafDataImpl};
use crate::storage::storage_trait::StorageValue;

impl LeafDataImpl {
    /// Serializes the leaf data into a byte vector.
    /// The serialization is done as follows:
    /// - For storage values: serializes the value into a 32-byte vector.
    /// - For compiled class hashes or state tree tuples: creates a  json string
    ///   describing the leaf and cast it into a byte vector.
    pub fn serialize(&self) -> StorageValue {
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
                // TODO(Aviv, 8/5/2024): Use height from the input.
                let json_string = format!(
                    r#"{{"contract_hash": "{}", "storage_commitment_tree": {{"root": "{}", "height": 252}}, "nonce": "{}"}}"#,
                    class_hash.0.to_fixed_hex_string(),
                    storage_root_hash.0.to_fixed_hex_string(),
                    nonce.0.to_hex(),
                );
                StorageValue(json_string.into_bytes())
            }
        }
    }
}
