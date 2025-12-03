use ethnum::U256;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    EdgePath,
    EdgePathLength,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, HasDynamicPrefix};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

pub(crate) const INDEX_LAYOUT_BINARY_BYTES: usize = 32;
pub(crate) const SERIALIZE_HASH_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexDbFilledNode<L: Leaf> {
    pub hash: HashOutput,
    pub data: IndexNodeData<L>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexNodeData<L: Leaf> {
    Binary,
    Edge(PathToBottom),
    Leaf(L),
}

pub trait IndexLayoutLeaf: Sized {
    fn serialize_index_layout(&self) -> DbValue;
    fn deserialize_index_layout(value: &DbValue) -> Result<Self, DeserializationError>;
}

impl<L: Leaf> HasDynamicPrefix for IndexDbFilledNode<L> {
    fn get_prefix(&self) -> DbKeyPrefix {
        DbKeyPrefix::new(&[])
    }
}

impl<L: Leaf + IndexLayoutLeaf> DBObject for IndexDbFilledNode<L> {
    fn serialize(&self) -> DbValue {
        match &self.data {
            IndexNodeData::Binary => DbValue(self.hash.0.to_bytes_be().to_vec()),
            IndexNodeData::Edge(path_to_bottom) => {
                let mut raw_bytes = self.hash.0.to_bytes_be().to_vec();
                let bit_len: u8 = path_to_bottom.length.into();
                let byte_len: usize = (bit_len.saturating_add(7) / 8).into();
                raw_bytes.push(bit_len);
                raw_bytes.extend(path_to_bottom.path.0.to_le_bytes()[..byte_len].to_vec());
                DbValue(raw_bytes)
            }
            IndexNodeData::Leaf(leaf) => leaf.serialize_index_layout(),
        }
    }
}

impl<L: Leaf + IndexLayoutLeaf> IndexDbFilledNode<L> {
    pub fn deserialize_inner_node(value: &DbValue) -> Result<Self, DeserializationError> {
        if value.0.len() == INDEX_LAYOUT_BINARY_BYTES {
            Ok(Self {
                hash: HashOutput(Felt::from_bytes_be_slice(&value.0)),
                data: IndexNodeData::Binary,
            })
        } else {
            let node_hash = HashOutput(Felt::from_bytes_be_slice(&value.0[..SERIALIZE_HASH_BYTES]));
            let value = &value.0[SERIALIZE_HASH_BYTES..];
            let bit_len = value[0];
            let byte_len = (bit_len.saturating_add(7) / 8).into();
            let path = U256::from_le_bytes(
                value[1..byte_len].try_into().expect("Slice with incorrect length."),
            );
            Ok(Self {
                hash: node_hash,
                data: IndexNodeData::Edge(
                    PathToBottom::new(
                        EdgePath(path),
                        EdgePathLength::new(bit_len)
                            .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                    )
                    .map_err(|error| DeserializationError::ValueError(Box::new(error)))?,
                ),
            })
        }
    }

    pub fn deserialize_leaf<Hasher: TreeHashFunction<L>>(
        value: &DbValue,
    ) -> Result<Self, DeserializationError> {
        let leaf = L::deserialize_index_layout(value)?;
        let hash = Hasher::compute_leaf_hash(&leaf);
        Ok(Self { hash, data: IndexNodeData::Leaf(leaf) })
    }
}
