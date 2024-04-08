use crate::hash::types::HashFunction;
use crate::patricia_merkle_tree::errors::SkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTree;
use crate::patricia_merkle_tree::types::{LeafDataTrait, TreeHashFunction};

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    /// Computes and returns the filled tree.
    fn compute_filled_tree(&self) -> Result<impl FilledTree<L>, SkeletonTreeError>;
}
