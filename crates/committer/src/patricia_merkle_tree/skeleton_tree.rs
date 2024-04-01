use std::iter::Map;

use crate::hash::types::HashFunction;
use crate::patricia_merkle_tree::errors::SkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTree;
use crate::patricia_merkle_tree::skeleton_node::SkeletonNode;
use crate::patricia_merkle_tree::types::{LeafTrait, NodeIndex, TreeHashFunction};

pub(crate) trait CurrentSkeletonTree<L: LeafTrait, H: HashFunction, TH: TreeHashFunction<L, H>> {
    /// Computes and returns updated skeleton tree.
    fn compute_updated_skeleton_tree(
        &self,
        index_to_updated_leaf: Map<NodeIndex, &SkeletonNode<L>>,
    ) -> Result<SkeletonTreeError, impl UpdatedSkeletonTree<L, H, TH>>;
}

pub(crate) trait UpdatedSkeletonTree<L: LeafTrait, H: HashFunction, TH: TreeHashFunction<L, H>> {
    /// Computes and returns the filled tree.
    fn compute_filled_tree(&self) -> Result<SkeletonTreeError, impl FilledTree<L>>;
}
