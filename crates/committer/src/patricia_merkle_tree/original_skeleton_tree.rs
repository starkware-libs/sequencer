use std::collections::HashMap;

use crate::hash::types::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex, TreeHashFunction, TreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::UpdatedSkeletonTree;
use crate::storage::storage_trait::Storage;

#[allow(dead_code)]
pub(crate) type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the Sibling
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait OriginalSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    fn compute_original_skeleton_tree(
        storage: impl Storage,
        leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<Box<Self>>;

    /// Computes and returns updated skeleton tree.
    fn compute_updated_skeleton_tree(
        &self,
        index_to_updated_leaf: HashMap<NodeIndex, L>,
    ) -> OriginalSkeletonTreeResult<impl UpdatedSkeletonTree<L, H, TH>>;
}
