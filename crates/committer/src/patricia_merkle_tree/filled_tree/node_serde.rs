use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData, EdgeData, EdgePath, EdgePathLength, NodeData, PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::LeafDataImpl;
use crate::storage::errors::{DeserializationError, SerializationError};
use crate::storage::serde_trait::{Deserializable, Serializable};
use crate::storage::storage_trait::{create_db_key, StorageKey, StoragePrefix, StorageValue};
use serde::{Deserialize, Serialize};

// Const describe the size of the serialized node.
pub(crate) const SERIALIZE_HASH_BYTES: usize = 32;
pub(crate) const BINARY_BYTES: usize = 2 * SERIALIZE_HASH_BYTES;
pub(crate) const EDGE_LENGTH_BYTES: usize = 1;
pub(crate) const EDGE_PATH_BYTES: usize = 32;
pub(crate) const EDGE_BYTES: usize = SERIALIZE_HASH_BYTES + EDGE_PATH_BYTES + EDGE_LENGTH_BYTES;
#[allow(dead_code)]
pub(crate) const STORAGE_LEAF_SIZE: usize = SERIALIZE_HASH_BYTES;

/// Temporary struct to serialize the leaf CompiledClass.
/// Required to comply to existing storage layout.
#[derive(Serialize, Deserialize)]
pub(crate) struct LeafCompiledClassToSerialize {
    pub(crate) compiled_class_hash: Felt,
}

/// Alias for serialization and deserialization results of filled nodes.
type FilledNodeSerializationResult = Result<StorageValue, SerializationError>;
type FilledNodeDeserializationResult = Result<FilledNode<LeafDataImpl>, DeserializationError>;

impl FilledNode<LeafDataImpl> {
    pub(crate) fn suffix(&self) -> [u8; SERIALIZE_HASH_BYTES] {
        self.hash.0.to_bytes_be()
    }
}

impl Serializable for FilledNode<LeafDataImpl> {
    /// This method serializes the filled node into a byte vector, where:
    /// - For binary nodes: Concatenates left and right hashes.
    /// - For edge nodes: Concatenates bottom hash, path, and path length.
    /// - For leaf nodes: use leaf.serialize() method.
    fn serialize(&self) -> FilledNodeSerializationResult {
        match &self.data {
            NodeData::Binary(BinaryData {
                left_hash,
                right_hash,
            }) => {
                // Serialize left and right hashes to byte arrays.
                let left: [u8; SERIALIZE_HASH_BYTES] = left_hash.0.to_bytes_be();
                let right: [u8; SERIALIZE_HASH_BYTES] = right_hash.0.to_bytes_be();

                // Concatenate left and right hashes.
                let serialized = [left, right].concat();
                Ok(StorageValue(serialized))
            }

            NodeData::Edge(EdgeData {
                bottom_hash,
                path_to_bottom,
            }) => {
                // Serialize bottom hash, path, and path length to byte arrays.
                let bottom: [u8; SERIALIZE_HASH_BYTES] = bottom_hash.0.to_bytes_be();
                let path: [u8; SERIALIZE_HASH_BYTES] = path_to_bottom.path.0.to_bytes_be();
                let length: [u8; 1] = path_to_bottom.length.0.to_be_bytes();

                // Concatenate bottom hash, path, and path length.
                let serialized = [bottom.to_vec(), path.to_vec(), length.to_vec()].concat();
                Ok(StorageValue(serialized))
            }

            NodeData::Leaf(leaf_data) => Ok(leaf_data.serialize()),
        }
    }

    /// Returns the db key of the filled node - [prefix + b":" + suffix].
    fn db_key(&self) -> StorageKey {
        let suffix = self.suffix();

        match &self.data {
            NodeData::Binary(_) | NodeData::Edge(_) => {
                create_db_key(StoragePrefix::InnerNode, &suffix)
            }
            NodeData::Leaf(LeafDataImpl::StorageValue(_)) => {
                create_db_key(StoragePrefix::StorageLeaf, &suffix)
            }
            NodeData::Leaf(LeafDataImpl::CompiledClassHash(_)) => {
                create_db_key(StoragePrefix::CompiledClassLeaf, &suffix)
            }
            NodeData::Leaf(LeafDataImpl::ContractState { .. }) => {
                create_db_key(StoragePrefix::StateTreeLeaf, &suffix)
            }
        }
    }
}

impl Deserializable for FilledNode<LeafDataImpl> {
    /// Deserializes non-leaf nodes; if a serialized leaf node is given, the hash
    /// is used but the data is ignored.
    fn deserialize(key: &StorageKey, value: &StorageValue) -> FilledNodeDeserializationResult {
        if value.0.len() == BINARY_BYTES {
            Ok(Self {
                hash: HashOutput(Felt::from_bytes_be_slice(&key.0)),
                data: NodeData::Binary(BinaryData {
                    left_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[..SERIALIZE_HASH_BYTES],
                    )),
                    right_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[SERIALIZE_HASH_BYTES..],
                    )),
                }),
            })
        } else if value.0.len() == EDGE_BYTES {
            return Ok(Self {
                hash: HashOutput(Felt::from_bytes_be_slice(&key.0)),
                data: NodeData::Edge(EdgeData {
                    bottom_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[..SERIALIZE_HASH_BYTES],
                    )),
                    path_to_bottom: PathToBottom {
                        path: EdgePath(Felt::from_bytes_be_slice(
                            &value.0[SERIALIZE_HASH_BYTES..SERIALIZE_HASH_BYTES + EDGE_PATH_BYTES],
                        )),
                        length: EdgePathLength(value.0[EDGE_BYTES - 1]),
                    },
                }),
            });
        } else {
            // TODO(Nimrod, 5/5/2024): See if deserializing leaves data is needed somewhere.
            return Ok(Self {
                hash: HashOutput(Felt::from_bytes_be_slice(&key.0)),
                // Dummy value which will be ignored.
                data: NodeData::Leaf(LeafDataImpl::StorageValue(Felt::ZERO)),
            });
        }
    }
}
