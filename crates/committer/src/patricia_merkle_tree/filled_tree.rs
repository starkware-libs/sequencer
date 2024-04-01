use std::iter::Map;

use crate::patricia_merkle_tree::filled_node::FilledNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex};

pub(crate) trait FilledTree<L: LeafDataTrait> {
    fn get_all_nodes(&self) -> Map<NodeIndex, &FilledNode<L>>;
}
