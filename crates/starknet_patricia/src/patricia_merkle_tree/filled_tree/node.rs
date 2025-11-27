use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree, complete with its hash and data.
pub struct FilledNode<L: Leaf> {
    pub hash: HashOutput,
    pub data: NodeData<L>,
}
