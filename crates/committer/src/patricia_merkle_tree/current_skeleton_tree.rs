use std::collections::HashMap;

use crate::hash::types::HashFunction;
use crate::patricia_merkle_tree::errors::SkeletonTreeError;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex, TreeHashFunction};
use crate::patricia_merkle_tree::updated_skeleton_tree::UpdatedSkeletonTree;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the Sibling
/// nodes on the Merkle paths from the updated leaves to the root.
pub(crate) trait CurrentSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    /// Computes and returns updated skeleton tree.
    fn compute_updated_skeleton_tree(
        &self,
        index_to_updated_leaf: HashMap<NodeIndex, L>,
    ) -> Result<impl UpdatedSkeletonTree<L, H, TH>, SkeletonTreeError>;
}
