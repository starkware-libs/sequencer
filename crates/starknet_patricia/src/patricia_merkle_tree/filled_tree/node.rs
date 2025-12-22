use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;

#[derive(Clone, Debug, PartialEq, Eq)]
/// A node in a Patricia-Merkle tree, complete with its hash and data.
pub struct FilledNode<L: Leaf, ChildData> {
    pub hash: HashOutput,
    pub data: NodeData<L, ChildData>,
}

// TODO(Ariel, 14/12/2025): move this type (along with DBObject impl) to the facts_db module in
// starknet_committer. This can happen after serialization of FilledTree is made generic in the
// layout.
pub type FactDbFilledNode<L> = FilledNode<L, HashOutput>;
