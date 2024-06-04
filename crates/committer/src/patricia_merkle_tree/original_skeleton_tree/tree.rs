use std::collections::HashMap;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::NodeIndex;

use crate::storage::storage_trait::Storage;

pub(crate) type OriginalSkeletonNodeMap = HashMap<NodeIndex, OriginalSkeletonNode>;
pub(crate) type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the Sibling
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait OriginalSkeletonTree {
    fn create(
        storage: &impl Storage,
        leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
    ) -> OriginalSkeletonTreeResult<Self>
    where
        Self: std::marker::Sized;

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap;

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap;
}

// TODO(Dori, 1/7/2024): Make this a tuple struct.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct OriginalSkeletonTreeImpl {
    pub(crate) nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
}

impl OriginalSkeletonTree for OriginalSkeletonTreeImpl {
    fn create(
        storage: &impl Storage,
        sorted_leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
    ) -> OriginalSkeletonTreeResult<Self> {
        Self::create_impl(storage, sorted_leaf_indices, root_hash)
    }

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap {
        &mut self.nodes
    }
}
