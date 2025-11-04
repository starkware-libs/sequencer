use starknet_api::hash::HashOutput;
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

/// Trait for hash functions.
pub trait HashFunction {
    /// Computes the hash of the given input.
    fn hash(left: &Felt, right: &Felt) -> HashOutput;
}

pub trait TreeHashFunction<L: Leaf> {
    /// Computes the hash of the given leaf.
    fn compute_leaf_hash(leaf_data: &L) -> HashOutput;

    /// Computes the hash for the given node data.
    fn compute_node_hash(node_data: &NodeData<L>) -> HashOutput;

    /// The default implementation for internal nodes is based on the following reference:
    /// <https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction>
    fn compute_node_hash_with_inner_hash_function<H: HashFunction>(
        node_data: &NodeData<L>,
    ) -> HashOutput {
        match node_data {
            NodeData::Binary(BinaryData { left_hash, right_hash }) => {
                H::hash(&left_hash.0, &right_hash.0)
            }
            NodeData::Edge(EdgeData {
                bottom_hash: hash_output,
                path_to_bottom: PathToBottom { path, length, .. },
            }) => HashOutput(H::hash(&hash_output.0, &Felt::from(path)).0 + Felt::from(*length)),
            NodeData::Leaf(leaf_data) => Self::compute_leaf_hash(leaf_data),
        }
    }
}
