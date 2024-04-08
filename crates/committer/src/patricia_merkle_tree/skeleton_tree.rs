use std::collections::HashMap;

use crate::hash::types::HashFunction;
use crate::patricia_merkle_tree::errors::SkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTree;
use crate::patricia_merkle_tree::skeleton_node::SkeletonNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex, TreeHashFunction};

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes of the Sibling nodes on the Merkle paths from the
/// updated leaves to the root.
pub(crate) trait CurrentSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    /// Computes and returns updated skeleton tree.
    fn compute_updated_skeleton_tree(
        &self,
        index_to_updated_leaf: HashMap<NodeIndex, &SkeletonNode<L>>,
    ) -> Result<impl UpdatedSkeletonTree<L, H, TH>, SkeletonTreeError>;
}

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    /// Computes and returns the filled tree.
    fn compute_filled_tree(&self) -> Result<impl FilledTree<L>, SkeletonTreeError>;
}
