use std::collections::HashMap;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::types::{NodeIndex, TreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;
use crate::storage::storage_trait::Storage;

#[allow(dead_code)]
pub(crate) type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the Sibling
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait OriginalSkeletonTree<L: LeafData + std::clone::Clone> {
    #[allow(dead_code)]
    fn create_tree(
        storage: &impl Storage,
        leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<Self>
    where
        Self: std::marker::Sized;

    #[allow(dead_code)]
    /// Computes and returns updated skeleton tree.
    fn compute_updated_skeleton_tree(
        &self,
        index_to_updated_leaf: HashMap<NodeIndex, L>,
    ) -> OriginalSkeletonTreeResult<impl UpdatedSkeletonTree<L>>;
}
