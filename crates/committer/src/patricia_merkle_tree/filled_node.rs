use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTreeResult;
use crate::patricia_merkle_tree::original_skeleton_tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::serialized_node::{
    LeafCompiledClassToSerialize, SerializeNode, COMPLIED_CLASS_PREFIX, INNER_NODE_PREFIX,
    SERIALIZE_HASH_BYTES, STATE_TREE_LEAF_PREFIX, STORAGE_LEAF_PREFIX,
};
use crate::patricia_merkle_tree::serialized_node::{BINARY_BYTES, EDGE_BYTES, EDGE_PATH_BYTES};
use crate::patricia_merkle_tree::types::{EdgeData, LeafDataTrait};
use crate::patricia_merkle_tree::types::{EdgePath, EdgePathLength, PathToBottom};
use crate::storage::storage_trait::{create_db_key, StorageKey, StorageValue};
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

#[derive(Clone, Debug, PartialEq, Eq)]
// A Patricia-Merkle tree node's data, i.e., the pre-image of its hash.
pub(crate) enum NodeData<L: LeafDataTrait> {
    Binary(BinaryData),
    Edge(EdgeData),
    Leaf(L),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BinaryData {
    pub(crate) left_hash: HashOutput,
    pub(crate) right_hash: HashOutput,
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

#[allow(dead_code)]
impl FilledNode<LeafData> {
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
}
