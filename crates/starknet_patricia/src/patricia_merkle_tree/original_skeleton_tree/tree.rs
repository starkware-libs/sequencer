use std::collections::HashMap;

use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};

pub type OriginalSkeletonNodeMap = HashMap<NodeIndex, OriginalSkeletonNode>;
pub type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the unmodified
/// nodes on the Merkle paths from the updated leaves to the root.
pub trait OriginalSkeletonTree<'a>: Sized {
    fn get_nodes(&self) -> &OriginalSkeletonNodeMap;

    #[allow(dead_code)]
    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a>;
}

// TODO(Dori, 1/7/2024): Make this a tuple struct.
#[derive(Debug, PartialEq)]
pub struct OriginalSkeletonTreeImpl<'a> {
    pub nodes: OriginalSkeletonNodeMap,
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
}

impl<'a> OriginalSkeletonTree<'a> for OriginalSkeletonTreeImpl<'a> {
    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a> {
        self.sorted_leaf_indices
    }
}

impl<'a> OriginalSkeletonTreeImpl<'a> {
    pub fn create_unmodified(root_hash: HashOutput) -> Self {
        Self {
            nodes: HashMap::from([(
                NodeIndex::ROOT,
                OriginalSkeletonNode::UnmodifiedSubTree(root_hash),
            )]),
            sorted_leaf_indices: SortedLeafIndices::default(),
        }
    }

    pub fn create_empty(sorted_leaf_indices: SortedLeafIndices<'a>) -> Self {
        Self { nodes: HashMap::new(), sorted_leaf_indices }
    }
}

/// Wraps an original skeleton node map and allows inserting additional nodes to the non-mutable
/// map. Used in the construction of the `UpdatedSkeletonTree`, where it is necessary
/// to temporarily add "fake" (i.e., placeholder) nodes to the original skeleton nodes
/// during tree construction.
pub struct ExtendedOriginalSkeletonNodes<'a> {
    original_nodes: &'a OriginalSkeletonNodeMap,
    additional_nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
}

impl<'a> ExtendedOriginalSkeletonNodes<'a> {
    pub fn new(tree: &'a impl OriginalSkeletonTree<'a>) -> Self {
        Self { original_nodes: tree.get_nodes(), additional_nodes: OriginalSkeletonNodeMap::new() }
    }

    pub fn insert(&mut self, index: NodeIndex, node: OriginalSkeletonNode) {
        self.additional_nodes.insert(index, node);
    }

    pub fn get(&self, index: &NodeIndex) -> Option<&OriginalSkeletonNode> {
        self.additional_nodes.get(index).or_else(|| self.original_nodes.get(index))
    }
}
