use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{BinaryData, FilledNode, LeafData, NodeData};
use crate::patricia_merkle_tree::filled_tree::tree::FilledTreeResult;
use crate::patricia_merkle_tree::original_skeleton_tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::{EdgeData, EdgePath, EdgePathLength, PathToBottom};
use crate::storage::storage_trait::{create_db_key, StorageKey, StorageValue};
use crate::types::Felt;
use serde::{Deserialize, Serialize};

// Const describe the size of the serialized node.
pub(crate) const SERIALIZE_HASH_BYTES: usize = 32;
#[allow(dead_code)]
pub(crate) const BINARY_BYTES: usize = 2 * SERIALIZE_HASH_BYTES;
#[allow(dead_code)]
pub(crate) const EDGE_LENGTH_BYTES: usize = 1;
#[allow(dead_code)]
pub(crate) const EDGE_PATH_BYTES: usize = 32;
#[allow(dead_code)]
pub(crate) const EDGE_BYTES: usize = SERIALIZE_HASH_BYTES + EDGE_PATH_BYTES + EDGE_LENGTH_BYTES;
#[allow(dead_code)]
pub(crate) const STORAGE_LEAF_SIZE: usize = SERIALIZE_HASH_BYTES;

// TODO(Aviv, 17/4/2024): add CompiledClassLeaf size.
// TODO(Aviv, 17/4/2024): add StateTreeLeaf size.

// Const describe the prefix of the serialized node.
pub(crate) const STORAGE_LEAF_PREFIX: &[u8; 21] = b"starknet_storage_leaf";
pub(crate) const STATE_TREE_LEAF_PREFIX: &[u8; 14] = b"contract_state";
pub(crate) const COMPLIED_CLASS_PREFIX: &[u8; 19] = b"contract_class_leaf";
pub(crate) const INNER_NODE_PREFIX: &[u8; 13] = b"patricia_node";

/// Enum to describe the serialized node.
#[allow(dead_code)]
pub(crate) enum SerializeNode {
    Binary(Vec<u8>),
    Edge(Vec<u8>),
    CompiledClassLeaf(Vec<u8>),
    StorageLeaf(Vec<u8>),
    StateTreeLeaf(Vec<u8>),
}

/// Temporary struct to serialize the leaf CompiledClass.
/// Required to comply to existing storage layout.
#[derive(Serialize, Deserialize)]
pub(crate) struct LeafCompiledClassToSerialize {
    pub(crate) compiled_class_hash: Felt,
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

    /// Returns the suffix of the filled node, represented by its hash as a byte array.
    #[allow(dead_code)]
    pub(crate) fn suffix(&self) -> [u8; SERIALIZE_HASH_BYTES] {
        self.hash.0.as_bytes()
    }

    /// Returns the db key of the filled node - [prefix + b":" + suffix].
    #[allow(dead_code)]
    pub(crate) fn db_key(&self) -> StorageKey {
        let suffix = self.suffix();

        match &self.data {
            NodeData::Binary(_) | NodeData::Edge(_) => create_db_key(INNER_NODE_PREFIX, &suffix),
            NodeData::Leaf(LeafData::StorageValue(_)) => {
                create_db_key(STORAGE_LEAF_PREFIX, &suffix)
            }
            NodeData::Leaf(LeafData::CompiledClassHash(_)) => {
                create_db_key(COMPLIED_CLASS_PREFIX, &suffix)
            }
            NodeData::Leaf(LeafData::StateTreeTuple { .. }) => {
                create_db_key(STATE_TREE_LEAF_PREFIX, &suffix)
            }
        }
    }

    /// Deserializes non-leaf nodes; if a serialized leaf node is given, the hash
    /// is used but the data is ignored.
    pub(crate) fn deserialize(
        key: &StorageKey,
        value: &StorageValue,
    ) -> OriginalSkeletonTreeResult<Self> {
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
                data: NodeData::Leaf(LeafData::StorageValue(Felt::ZERO)),
            });
        }
    }
}
