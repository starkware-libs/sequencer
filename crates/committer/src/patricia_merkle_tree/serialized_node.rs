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
