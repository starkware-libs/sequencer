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
