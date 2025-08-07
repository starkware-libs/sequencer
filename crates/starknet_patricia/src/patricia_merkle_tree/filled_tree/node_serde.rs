use ethnum::U256;
use serde::{Deserialize, Serialize};
use starknet_patricia_storage::db_object::{DBObject, HasDynamicPrefix};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKey, DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

// Const describe the size of the serialized node.
pub(crate) const SERIALIZE_HASH_BYTES: usize = 32;
pub(crate) const BINARY_BYTES: usize = 2 * SERIALIZE_HASH_BYTES;
pub(crate) const EDGE_LENGTH_BYTES: usize = 1;
pub(crate) const EDGE_PATH_BYTES: usize = 32;
pub(crate) const EDGE_BYTES: usize = SERIALIZE_HASH_BYTES + EDGE_PATH_BYTES + EDGE_LENGTH_BYTES;
#[allow(dead_code)]
pub(crate) const STORAGE_LEAF_SIZE: usize = SERIALIZE_HASH_BYTES;

#[derive(Debug)]
pub enum PatriciaPrefix {
    InnerNode,
    Leaf(DbKeyPrefix),
}

impl From<PatriciaPrefix> for DbKeyPrefix {
    fn from(value: PatriciaPrefix) -> Self {
        match value {
            PatriciaPrefix::InnerNode => Self::new(b"patricia_node"),
            PatriciaPrefix::Leaf(prefix) => prefix,
        }
    }
}

/// Temporary struct to serialize the leaf CompiledClass.
/// Required to comply to existing storage layout.
#[derive(Serialize, Deserialize)]
pub(crate) struct LeafCompiledClassToSerialize {
    pub(crate) compiled_class_hash: Felt,
}

impl<L: Leaf> FilledNode<L> {
    pub fn suffix(&self) -> [u8; SERIALIZE_HASH_BYTES] {
        self.hash.0.to_bytes_be()
    }

    pub fn db_key(&self) -> DbKey {
        self.get_db_key(&self.suffix())
    }
}

impl<L: Leaf> HasDynamicPrefix for FilledNode<L> {
    fn get_prefix(&self) -> DbKeyPrefix {
        match &self.data {
            NodeData::Binary(_) | NodeData::Edge(_) => PatriciaPrefix::InnerNode,
            NodeData::Leaf(_) => PatriciaPrefix::Leaf(L::get_static_prefix()),
        }
        .into()
    }
}

impl<L: Leaf> DBObject for FilledNode<L> {
    /// This method serializes the filled node into a byte vector, where:
    /// - For binary nodes: Concatenates left and right hashes.
    /// - For edge nodes: Concatenates bottom hash, path, and path length.
    /// - For leaf nodes: use leaf.serialize() method.
    fn serialize(&self) -> DbValue {
        match &self.data {
            NodeData::Binary(BinaryData { left_hash, right_hash }) => {
                // Serialize left and right hashes to byte arrays.
                let left: [u8; SERIALIZE_HASH_BYTES] = left_hash.0.to_bytes_be();
                let right: [u8; SERIALIZE_HASH_BYTES] = right_hash.0.to_bytes_be();

                // Concatenate left and right hashes.
                let serialized = [left, right].concat();
                DbValue(serialized)
            }

            NodeData::Edge(EdgeData { bottom_hash, path_to_bottom }) => {
                // Serialize bottom hash, path, and path length to byte arrays.
                let bottom: [u8; SERIALIZE_HASH_BYTES] = bottom_hash.0.to_bytes_be();
                let path: [u8; SERIALIZE_HASH_BYTES] =
                    U256::from(&path_to_bottom.path).to_be_bytes();
                let length: [u8; 1] = u8::from(path_to_bottom.length).to_be_bytes();

                // Concatenate bottom hash, path, and path length.
                let serialized = [bottom.to_vec(), path.to_vec(), length.to_vec()].concat();
                DbValue(serialized)
            }

            NodeData::Leaf(leaf_data) => leaf_data.serialize(),
        }
    }
}

impl<L: Leaf> FilledNode<L> {
    /// Deserializes filled nodes.
    pub fn deserialize(
        node_hash: HashOutput,
        value: &DbValue,
        is_leaf: bool,
    ) -> Result<Self, DeserializationError> {
        if is_leaf {
            return Ok(Self { hash: node_hash, data: NodeData::Leaf(L::deserialize(value)?) });
        }

        if value.0.len() == BINARY_BYTES {
            Ok(Self {
                hash: node_hash,
                data: NodeData::Binary(BinaryData {
                    left_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[..SERIALIZE_HASH_BYTES],
                    )),
                    right_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[SERIALIZE_HASH_BYTES..],
                    )),
                }),
            })
        } else {
            assert_eq!(
                value.0.len(),
                EDGE_BYTES,
                "Unexpected inner node storage value length {}, expected to be {} or {}.",
                value.0.len(),
                EDGE_BYTES,
                BINARY_BYTES
            );
            Ok(Self {
                hash: node_hash,
                data: NodeData::Edge(EdgeData {
                    bottom_hash: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[..SERIALIZE_HASH_BYTES],
                    )),
                    path_to_bottom: PathToBottom::new(
                        U256::from_be_bytes(
                            value.0[SERIALIZE_HASH_BYTES..SERIALIZE_HASH_BYTES + EDGE_PATH_BYTES]
                                .try_into()
                                .expect("Slice with incorrect length."),
                        )
                        .into(),
                        EdgePathLength::new(value.0[EDGE_BYTES - 1])
                            .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                    )
                    .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                }),
            })
        }
    }
}
