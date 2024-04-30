use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;

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
pub(crate) struct FilledNode<L: LeafData> {
    pub(crate) hash: HashOutput,
    pub(crate) data: NodeData<L>,
}
