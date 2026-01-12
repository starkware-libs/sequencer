use std::marker::PhantomData;

use ethnum::U256;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::{SubTreeTrait, UnmodifiedChildTraversal};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{
    DBObject,
    EmptyDeserializationContext,
    HasStaticPrefix,
};
use starknet_patricia_storage::errors::{DeserializationError, SerializationResult};
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::db::facts_db::node_serde::SERIALIZE_HASH_BYTES;
use crate::db::index_db::leaves::INDEX_LAYOUT_DB_KEY_SEPARATOR;
use crate::hash_function::hash::TreeHashFunctionImpl;

// In index layout, for binary nodes, only the hash is stored.
pub(crate) const INDEX_LAYOUT_BINARY_BYTES: usize = SERIALIZE_HASH_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyNodeData;

impl From<HashOutput> for EmptyNodeData {
    fn from(_hash: HashOutput) -> Self {
        Self
    }
}

/// A filled node in the index layout, parameterized by the leaf type and hash function.
/// The hash function is used during deserialization to compute the leaf hash (which is not stored).
#[derive(PartialEq, Debug, derive_more::Into)]
pub struct IndexFilledNodeWithHasher<L: Leaf, H>(
    pub FilledNode<L, EmptyNodeData>,
    #[into(skip)] pub PhantomData<H>,
);

impl<L: Leaf, H> IndexFilledNodeWithHasher<L, H> {
    pub fn new(filled_node: FilledNode<L, EmptyNodeData>) -> Self {
        Self(filled_node, PhantomData)
    }
}

/// Type alias for `IndexFilledNodeWithHasher` with the production hash function.
pub type IndexFilledNode<L> = IndexFilledNodeWithHasher<L, TreeHashFunctionImpl>;

pub struct IndexNodeContext {
    pub is_leaf: bool,
}

impl<L: Leaf, H> HasStaticPrefix for IndexFilledNodeWithHasher<L, H> {
    type KeyContext = <L as HasStaticPrefix>::KeyContext;
    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix {
        L::get_static_prefix(key_context)
    }
}

impl<L, H> DBObject for IndexFilledNodeWithHasher<L, H>
where
    L: Leaf,
    H: TreeHashFunction<L>,
{
    const DB_KEY_SEPARATOR: &[u8] = INDEX_LAYOUT_DB_KEY_SEPARATOR;

    type DeserializeContext = IndexNodeContext;

    fn serialize(&self) -> SerializationResult<DbValue> {
        match &self.0.data {
            // Binary node - only serialize the hash.
            NodeData::Binary(_) => Ok(DbValue(self.0.hash.0.to_bytes_be().to_vec())),

            // Edge node serialization format: hash (32 bytes) | path-length (1 byte) | path
            // (without leading zeros, <= 32 bytes).
            NodeData::Edge(edge_data) => {
                let mut raw_bytes = self.0.hash.0.to_bytes_be().to_vec();
                // Bit length of the path.
                let bit_len: u8 = edge_data.path_to_bottom.length.into();
                let byte_len = byte_len_from_bit_len(bit_len);
                raw_bytes.push(bit_len);
                // Add the path bytes without leading zeros.
                raw_bytes
                    .extend(edge_data.path_to_bottom.path.0.to_le_bytes()[..byte_len].to_vec());
                Ok(DbValue(raw_bytes))
            }
            NodeData::Leaf(leaf) => leaf.serialize(),
        }
    }

    fn deserialize(
        value: &DbValue,
        deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        if deserialize_context.is_leaf {
            let leaf = L::deserialize(value, &EmptyDeserializationContext)?;
            let hash = H::compute_leaf_hash(&leaf);
            Ok(Self(FilledNode { hash, data: NodeData::Leaf(leaf) }, PhantomData))
        } else if value.0.len() == INDEX_LAYOUT_BINARY_BYTES {
            Ok(Self(
                FilledNode {
                    hash: HashOutput(Felt::from_bytes_be_slice(&value.0)),
                    data: NodeData::Binary(BinaryData {
                        left_data: EmptyNodeData,
                        right_data: EmptyNodeData,
                    }),
                },
                PhantomData,
            ))
        }
        // Edge nodes are always serailized to more than INDEX_LAYOUT_BINARY_BYTES bytes.
        else {
            let mut position = 0;
            let node_hash = HashOutput(Felt::from_bytes_be_slice(
                value
                    .0
                    .get(position..position + SERIALIZE_HASH_BYTES)
                    .expect("Unable to read node hash from db value"),
            ));
            position += SERIALIZE_HASH_BYTES;

            let bit_len = *value.0.get(position).expect("Unable to read path length from db value");
            position += 1;

            let byte_len = byte_len_from_bit_len(bit_len);

            let path_slice = value
                .0
                .get(position..position + byte_len)
                .expect("Unable to read path from db value");

            let mut buf = [0u8; 32];
            buf[..byte_len].copy_from_slice(path_slice);
            let path = U256::from_le_bytes(buf);

            Ok(Self(
                FilledNode {
                    hash: node_hash,
                    data: NodeData::Edge(EdgeData {
                        bottom_data: EmptyNodeData,
                        path_to_bottom: PathToBottom::new(
                            EdgePath(path),
                            EdgePathLength::new(bit_len).map_err(|error| {
                                DeserializationError::ValueError(Box::new(error))
                            })?,
                        )
                        .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                    }),
                },
                PhantomData,
            ))
        }
    }
}

fn byte_len_from_bit_len(bit_len: u8) -> usize {
    bit_len.div_ceil(8).into()
}

pub struct IndexLayoutSubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
}

impl<'a> SubTreeTrait<'a> for IndexLayoutSubTree<'a> {
    type NodeData = EmptyNodeData;
    type NodeDeserializeContext = IndexNodeContext;

    fn create(
        sorted_leaf_indices: SortedLeafIndices<'a>,
        root_index: NodeIndex,
        _child_data: Self::NodeData,
    ) -> Self {
        Self { sorted_leaf_indices, root_index }
    }

    fn get_root_index(&self) -> NodeIndex {
        self.root_index
    }

    fn get_sorted_leaf_indices(&self) -> &SortedLeafIndices<'a> {
        &self.sorted_leaf_indices
    }

    fn should_traverse_unmodified_child(_data: Self::NodeData) -> UnmodifiedChildTraversal {
        // In index layout, to obtain a child hash for an `OriginalSkeletonNode`, we need to read
        // the child from the DB (this is not true in facts layout, where a node stores its children
        // hashes).
        UnmodifiedChildTraversal::Traverse
    }

    fn get_root_context(&self) -> Self::NodeDeserializeContext {
        Self::NodeDeserializeContext { is_leaf: self.is_leaf() }
    }

    fn get_root_db_key<L: Leaf>(&self, key_context: &<L as HasStaticPrefix>::KeyContext) -> DbKey {
        let prefix = L::get_static_prefix(key_context);
        let suffix = self.root_index.0.to_be_bytes();
        create_db_key(prefix, INDEX_LAYOUT_DB_KEY_SEPARATOR, &suffix)
    }
}
