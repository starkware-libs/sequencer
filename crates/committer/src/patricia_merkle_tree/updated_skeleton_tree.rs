use std::collections::HashMap;

use crate::hash::types::HashFunction;
use crate::patricia_merkle_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTree;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex, TreeHashFunction};
use crate::patricia_merkle_tree::updated_skeleton_node::UpdatedSkeletonNode;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>>
{
    /// Computes and returns the filled tree.
    fn compute_filled_tree(&self) -> Result<impl FilledTree<L>, UpdatedSkeletonTreeError>;
}

#[allow(dead_code)]
struct UpdatedSkeletonTreeImpl<L: LeafDataTrait, H: HashFunction, TH: TreeHashFunction<L, H>> {
    skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<L>>,
    hash_function: H,
    tree_hash_function: TH,
}

#[allow(dead_code)]
impl<
        L: LeafDataTrait + std::clone::Clone + std::marker::Sync + std::marker::Send,
        H: HashFunction + std::marker::Sync,
        TH: TreeHashFunction<L, H> + std::marker::Sync,
    > UpdatedSkeletonTreeImpl<L, H, TH>
{
    fn get_sk_tree(&self) -> &HashMap<NodeIndex, UpdatedSkeletonNode<L>> {
        &self.skeleton_tree
    }

    fn get_node(
        &self,
        index: NodeIndex,
    ) -> Result<&UpdatedSkeletonNode<L>, UpdatedSkeletonTreeError> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode),
        }
    }
}
