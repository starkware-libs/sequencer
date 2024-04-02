use std::collections::HashMap;

use crate::patricia_merkle_tree::filled_node::FilledNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex};

pub(crate) trait FilledTree<L: LeafDataTrait> {
    fn get_all_nodes(&self) -> HashMap<NodeIndex, &FilledNode<L>>;
}
