use crate::patricia_merkle_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTreeResult;
use crate::patricia_merkle_tree::serialized_node::{
    LeafCompiledClassToSerialize, SerializeNode, SERIALIZE_HASH_BYTES,
};
use crate::patricia_merkle_tree::types::{EdgeData, LeafDataTrait};
use crate::{hash::hash_trait::HashOutput, types::Felt};
// TODO(Nimrod, 1/6/2024): Swap to starknet-types-core types once implemented.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClassHash(pub Felt);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Nonce(pub Felt);

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree which was modified during an update.
pub(crate) struct FilledNode<L: LeafDataTrait> {
    pub(crate) hash: HashOutput,
    pub(crate) data: NodeData<L>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
// A Patricia-Merkle tree node's data, i.e., the pre-image of its hash.
pub(crate) enum NodeData<L: LeafDataTrait> {
    Binary(BinaryData),
    Edge(EdgeData),
    Leaf(L),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BinaryData {
    pub(crate) left_hash: HashOutput,
    pub(crate) right_hash: HashOutput,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl FilledNode<LeafData> {
    /// This method serializes the filled node into a byte vector, where:
    /// - For binary nodes: Concatenates left and right hashes.
    /// - For edge nodes: Concatenates bottom hash, path, and path length.
    /// - For leaf nodes: use leaf.serialize() method.
    #[allow(dead_code)]
    pub(crate) fn serialize(&self) -> FilledTreeResult<SerializeNode> {
        match &self.data {
            NodeData::Binary(BinaryData {
                left_hash,
                right_hash,
            }) => {
                // Serialize left and right hashes to byte arrays.
                let left: [u8; SERIALIZE_HASH_BYTES] = left_hash.0.as_bytes();
                let right: [u8; SERIALIZE_HASH_BYTES] = right_hash.0.as_bytes();

                // Concatenate left and right hashes.
                let serialized = [left, right].concat();
                Ok(SerializeNode::Binary(serialized))
            }

            NodeData::Edge(EdgeData {
                bottom_hash,
                path_to_bottom,
            }) => {
                // Serialize bottom hash, path, and path length to byte arrays.
                let bottom: [u8; SERIALIZE_HASH_BYTES] = bottom_hash.0.as_bytes();
                let path: [u8; SERIALIZE_HASH_BYTES] = path_to_bottom.path.0.as_bytes();
                let length: [u8; 1] = path_to_bottom.length.0.to_be_bytes();

                // Concatenate bottom hash, path, and path length.
                let serialized = [bottom.to_vec(), path.to_vec(), length.to_vec()].concat();
                Ok(SerializeNode::Edge(serialized))
            }

            NodeData::Leaf(leaf_data) => leaf_data.serialize(),
        }
    }
}
