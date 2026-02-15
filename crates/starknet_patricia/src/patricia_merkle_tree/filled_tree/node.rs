use starknet_api::hash::HashOutput;
use starknet_rust_core::types::MerkleNode;

use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree, complete with its hash and data.
pub struct FilledNode<L: Leaf, ChildData> {
    pub hash: HashOutput,
    pub data: NodeData<L, ChildData>,
}

/// A node in an updated trie, where all the hashes were computed. Used in the `FilledTree` trait.
pub type HashFilledNode<L> = FilledNode<L, HashOutput>;

impl<L: Leaf> From<(HashOutput, &MerkleNode)> for FactDbFilledNode<L> {
    fn from((hash, node): (HashOutput, &MerkleNode)) -> Self {
        let data: NodeData<L, HashOutput> = NodeData::from(node);
        Self { hash, data }
    }
}
