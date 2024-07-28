use starknet_types_core::felt::FromStrError;

use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::impl_from_hex_for_felt_wrapper;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::types::NodeIndex;

// TODO(Nimrod, 1/6/2024): Use the ClassHash defined in starknet-types-core when available.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassHash(pub Felt);

impl From<&ClassHash> for NodeIndex {
    fn from(class_hash: &ClassHash) -> NodeIndex {
        NodeIndex::from_leaf_felt(&class_hash.0)
    }
}

impl_from_hex_for_felt_wrapper!(ClassHash);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Nonce(pub Felt);

impl_from_hex_for_felt_wrapper!(Nonce);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompiledClassHash(pub Felt);

impl_from_hex_for_felt_wrapper!(CompiledClassHash);

#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree which was modified during an update.
pub struct FilledNode<L: Leaf> {
    pub hash: HashOutput,
    pub data: NodeData<L>,
}
