use std::collections::HashMap;

use crate::patricia_merkle_tree::filled_node::FilledNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex};

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub(crate) trait FilledTree<L: LeafDataTrait> {
    fn get_all_nodes(&self) -> HashMap<NodeIndex, &FilledNode<L>>;
}
