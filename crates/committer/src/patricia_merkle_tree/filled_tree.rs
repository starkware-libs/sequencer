use std::collections::HashMap;
use std::sync::Mutex;

use crate::patricia_merkle_tree::filled_node::FilledNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex};

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub(crate) trait FilledTree<L: LeafDataTrait> {
    fn get_all_nodes(&self) -> &HashMap<NodeIndex, Mutex<FilledNode<L>>>;
}

pub(crate) struct FilledTreeImpl<L: LeafDataTrait> {
    tree_map: HashMap<NodeIndex, Mutex<FilledNode<L>>>,
}

#[allow(dead_code)]
impl<L: LeafDataTrait> FilledTreeImpl<L> {
    pub(crate) fn new(tree_map: HashMap<NodeIndex, Mutex<FilledNode<L>>>) -> Self {
        Self { tree_map }
    }
}

impl<L: LeafDataTrait> FilledTree<L> for FilledTreeImpl<L> {
    fn get_all_nodes(&self) -> &HashMap<NodeIndex, Mutex<FilledNode<L>>> {
        &self.tree_map
    }
}
