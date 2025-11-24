use std::collections::HashMap;

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

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap;

    #[allow(dead_code)]
    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a>;
}

// TODO(Dori, 1/7/2024): Make this a tuple struct.
#[derive(Debug, PartialEq)]
pub struct OriginalSkeletonTreeImpl<'a> {
    pub nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
}

impl<'a> OriginalSkeletonTree<'a> for OriginalSkeletonTreeImpl<'a> {
    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap {
        &mut self.nodes
    }

    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a> {
        self.sorted_leaf_indices
    }
}
