use ethnum::U256;
use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::{
    DBObject,
    EmptyDeserializationContext,
    HasDynamicPrefix,
    HasStaticPrefix,
};
use starknet_patricia_storage::errors::{DeserializationError, SerializationResult};
use starknet_patricia_storage::storage_trait::{DbKey, DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::filled_tree::node::{FactDbFilledNode, FilledNode};
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

// Const describe the size of the serialized node.
pub const SERIALIZE_HASH_BYTES: usize = 32;
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
            PatriciaPrefix::InnerNode => Self::new(b"patricia_node".into()),
            PatriciaPrefix::Leaf(prefix) => prefix,
        }
    }
}

// TODO(Ariel, 14/12/2025): generalize this to both layouts (e.g. via a new trait). ATM db_key is
// only used in the filled tree serialize function, which assumes facts layout.
impl<L: Leaf> FactDbFilledNode<L> {
    pub fn suffix(&self) -> [u8; SERIALIZE_HASH_BYTES] {
        self.hash.0.to_bytes_be()
    }

    pub fn db_key(&self, key_context: &<L as HasStaticPrefix>::KeyContext) -> DbKey {
        self.get_db_key(key_context, &self.suffix())
    }
}

impl<L: Leaf> HasDynamicPrefix for FilledNode<L, HashOutput> {
    // Inherit the KeyContext from the HasStaticPrefix implementation of the leaf.
    type KeyContext = <L as HasStaticPrefix>::KeyContext;

    fn get_prefix(&self, key_context: &Self::KeyContext) -> DbKeyPrefix {
        match &self.data {
            NodeData::Binary(_) | NodeData::Edge(_) => PatriciaPrefix::InnerNode,
            NodeData::Leaf(_) => PatriciaPrefix::Leaf(L::get_static_prefix(key_context)),
        }
        .into()
    }
}

/// Extra context required to deserialize [FilledNode<L, HashOutput>].
/// See [DBObject::DeserializeContext] for more information
pub struct FactNodeDeserializationContext {
    pub is_leaf: bool,
    pub node_hash: HashOutput,
}

impl<L: Leaf> DBObject for FactDbFilledNode<L> {
    type DeserializeContext = FactNodeDeserializationContext;
    /// This method serializes the filled node into a byte vector, where:
    /// - For binary nodes: Concatenates left and right hashes.
    /// - For edge nodes: Concatenates bottom hash, path, and path length.
    /// - For leaf nodes: use leaf.serialize() method.
    fn serialize(&self) -> SerializationResult<DbValue> {
        match &self.data {
            NodeData::Binary(BinaryData { left_data: left_hash, right_data: right_hash }) => {
                // Serialize left and right hashes to byte arrays.
                let left: [u8; SERIALIZE_HASH_BYTES] = left_hash.0.to_bytes_be();
                let right: [u8; SERIALIZE_HASH_BYTES] = right_hash.0.to_bytes_be();

                // Concatenate left and right hashes.
                let serialized = [left, right].concat();
                Ok(DbValue(serialized))
            }

            NodeData::Edge(EdgeData { bottom_data: bottom_hash, path_to_bottom }) => {
                // Serialize bottom hash, path, and path length to byte arrays.
                let bottom: [u8; SERIALIZE_HASH_BYTES] = bottom_hash.0.to_bytes_be();
                let path: [u8; SERIALIZE_HASH_BYTES] =
                    U256::from(&path_to_bottom.path).to_be_bytes();
                let length: [u8; 1] = u8::from(path_to_bottom.length).to_be_bytes();

                // Concatenate bottom hash, path, and path length.
                let serialized = [bottom.to_vec(), path.to_vec(), length.to_vec()].concat();
                Ok(DbValue(serialized))
            }

            NodeData::Leaf(leaf_data) => leaf_data.serialize(),
        }
    }

    fn deserialize(
        value: &DbValue,
        deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        if deserialize_context.is_leaf {
            return Ok(Self {
                hash: deserialize_context.node_hash,
                data: NodeData::Leaf(L::deserialize(value, &EmptyDeserializationContext)?),
            });
        }

        if value.0.len() == BINARY_BYTES {
            Ok(Self {
                hash: deserialize_context.node_hash,
                data: NodeData::Binary(BinaryData {
                    left_data: HashOutput(Felt::from_bytes_be_slice(
                        &value.0[..SERIALIZE_HASH_BYTES],
                    )),
                    right_data: HashOutput(Felt::from_bytes_be_slice(
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
                hash: deserialize_context.node_hash,
                data: NodeData::Edge(EdgeData {
                    bottom_data: HashOutput(Felt::from_bytes_be_slice(
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
