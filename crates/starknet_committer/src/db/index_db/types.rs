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
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::hash_function::hash::TreeHashFunctionImpl;

/// Number of bytes required to serialize a felt.
pub(crate) const SERIALIZE_HASH_BYTES: usize = 32;

// In index layout, for binary nodes, only the hash is stored.
pub(crate) const INDEX_LAYOUT_BINARY_BYTES: usize = SERIALIZE_HASH_BYTES;

#[derive(PartialEq, Debug)]
pub struct IndexFilledNode<L: Leaf>(pub FilledNode<L, ()>);

pub struct IndexNodeContext {
    pub is_leaf: bool,
}

impl<L: Leaf> HasStaticPrefix for IndexFilledNode<L> {
    type KeyContext = <L as HasStaticPrefix>::KeyContext;
    fn get_static_prefix(key_context: &Self::KeyContext) -> DbKeyPrefix {
        L::get_static_prefix(key_context)
    }
}

impl<L> DBObject for IndexFilledNode<L>
where
    L: Leaf,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    type DeserializeContext = IndexNodeContext;

    fn serialize(&self) -> SerializationResult<DbValue> {
        match &self.0.data {
            NodeData::Binary(_) => Ok(DbValue(self.0.hash.0.to_bytes_be().to_vec())),
            NodeData::Edge(edge_data) => {
                let mut raw_bytes = self.0.hash.0.to_bytes_be().to_vec();
                let bit_len: u8 = edge_data.path_to_bottom.length.into();
                let byte_len: usize = (bit_len.saturating_add(7) / 8).into();
                raw_bytes.push(bit_len);
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
            let hash = TreeHashFunctionImpl::compute_leaf_hash(&leaf);
            Ok(Self(FilledNode { hash, data: NodeData::Leaf(leaf) }))
        } else if value.0.len() == INDEX_LAYOUT_BINARY_BYTES {
            Ok(Self(FilledNode {
                hash: HashOutput(Felt::from_bytes_be_slice(&value.0)),
                data: NodeData::Binary(BinaryData::<()> { left_data: (), right_data: () }),
            }))
        } else {
            let node_hash = HashOutput(Felt::from_bytes_be_slice(&value.0[..SERIALIZE_HASH_BYTES]));
            let value = &value.0[SERIALIZE_HASH_BYTES..];
            let bit_len = value[0];
            let byte_len = (bit_len.saturating_add(7) / 8).into();

            let mut buf = [0u8; 32];
            buf[..byte_len].copy_from_slice(&value[1..byte_len + 1]);
            let path = U256::from_le_bytes(buf);

            Ok(Self(FilledNode {
                hash: node_hash,
                data: NodeData::Edge(EdgeData::<()> {
                    bottom_data: (),
                    path_to_bottom: PathToBottom::new(
                        EdgePath(path),
                        EdgePathLength::new(bit_len)
                            .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                    )
                    .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                }),
            }))
        }
    }
}

pub struct IndexLayoutSubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
}

impl<'a> SubTreeTrait<'a> for IndexLayoutSubTree<'a> {
    type NodeData = ();
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
        UnmodifiedChildTraversal::Traverse
    }

    fn get_root_context(&self) -> Self::NodeDeserializeContext {
        Self::NodeDeserializeContext { is_leaf: self.is_leaf() }
    }

    fn get_root_prefix<L: Leaf>(
        &self,
        key_context: &<L as HasStaticPrefix>::KeyContext,
    ) -> DbKeyPrefix {
        L::get_static_prefix(key_context)
    }

    fn get_root_suffix(&self) -> Vec<u8> {
        self.root_index.0.to_be_bytes().to_vec()
    }
}
