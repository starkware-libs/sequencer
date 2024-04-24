use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node_serde::{
    LeafCompiledClassToSerialize, SerializeNode,
};
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::types::LeafDataTrait;
use crate::types::Felt;

// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClassHash(pub Felt);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Nonce(pub Felt);

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CompiledClassHash(pub Felt);

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree which was modified during an update.
pub(crate) struct FilledNode<L: LeafDataTrait> {
    pub(crate) hash: HashOutput,
    pub(crate) data: NodeData<L>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LeafData {
    StorageValue(Felt),
    CompiledClassHash(ClassHash),
    StateTreeTuple {
        class_hash: ClassHash,
        contract_state_root_hash: Felt,
        nonce: Nonce,
    },
}

impl LeafDataTrait for LeafData {
    fn is_empty(&self) -> bool {
        match self {
            LeafData::StorageValue(value) => *value == Felt::ZERO,
            LeafData::CompiledClassHash(class_hash) => class_hash.0 == Felt::ZERO,
            LeafData::StateTreeTuple {
                class_hash,
                contract_state_root_hash,
                nonce,
            } => {
                nonce.0 == Felt::ZERO
                    && class_hash.0 == Felt::ZERO
                    && *contract_state_root_hash == Felt::ZERO
            }
        }
    }
}

impl LeafData {
    /// Serializes the leaf data into a byte vector.
    /// The serialization is done as follows:
    /// - For storage values: serializes the value into a 32-byte vector.
    /// - For compiled class hashes or state tree tuples: creates a  json string
    ///   describing the leaf and cast it into a byte vector.
    pub(crate) fn serialize(&self) -> Result<SerializeNode, FilledTreeError> {
        match &self {
            LeafData::StorageValue(value) => {
                Ok(SerializeNode::StorageLeaf(value.as_bytes().to_vec()))
            }

            LeafData::CompiledClassHash(class_hash) => {
                // Create a temporary object to serialize the leaf into a JSON.
                let temp_object_to_json = LeafCompiledClassToSerialize {
                    compiled_class_hash: class_hash.0,
                };

                // Serialize the leaf into a JSON.
                let json = serde_json::to_string(&temp_object_to_json)?;

                // Serialize the json into a byte vector.
                Ok(SerializeNode::CompiledClassLeaf(
                    json.into_bytes().to_owned(),
                ))
            }

            LeafData::StateTreeTuple { .. } => {
                todo!("implement.");
            }
        }
    }
}
