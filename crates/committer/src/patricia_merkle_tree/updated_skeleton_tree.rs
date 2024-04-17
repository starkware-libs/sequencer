use std::collections::HashMap;
use std::sync::Mutex;

use crate::hash::hash_trait::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::FilledTree;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex, TreeHashFunction};
use crate::patricia_merkle_tree::updated_skeleton_node::UpdatedSkeletonNode;
use crate::types::Felt;

use crate::patricia_merkle_tree::filled_node::{BinaryData, FilledNode, NodeData};
use crate::patricia_merkle_tree::filled_tree::FilledTreeImpl;
use crate::patricia_merkle_tree::types::EdgeData;

#[cfg(test)]
#[path = "updated_skeleton_tree_test.rs"]
pub mod updated_skeleton_tree_test;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree<L: LeafDataTrait + std::clone::Clone> {
    /// Computes and returns the filled tree.
    fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
    ) -> Result<impl FilledTree<L>, UpdatedSkeletonTreeError>;
}

struct UpdatedSkeletonTreeImpl<L: LeafDataTrait + std::clone::Clone> {
    skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<L>>,
}

impl<L: LeafDataTrait + std::clone::Clone> UpdatedSkeletonTreeImpl<L> {
    fn get_node(
        &self,
        index: NodeIndex,
    ) -> Result<&UpdatedSkeletonNode<L>, UpdatedSkeletonTreeError> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode),
        }
    }

    fn compute_filled_tree_rec<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
        index: NodeIndex,
        output_map: &mut HashMap<NodeIndex, Mutex<FilledNode<L>>>,
    ) -> Result<HashOutput, UpdatedSkeletonTreeError> {
        let node = self.get_node(index)?;
        match node {
            UpdatedSkeletonNode::Binary => {
                let left_index = NodeIndex(index.0 * Felt::TWO);
                let right_index = NodeIndex(left_index.0 + Felt::ONE);

                let left_hash = self.compute_filled_tree_rec::<H, TH>(left_index, output_map)?;
                let right_hash = self.compute_filled_tree_rec::<H, TH>(right_index, output_map)?;

                let data = NodeData::Binary(BinaryData {
                    left_hash,
                    right_hash,
                });
                let hash_value = TH::compute_node_hash(&data);
                output_map.insert(
                    index,
                    Mutex::new(FilledNode {
                        hash: hash_value,
                        data,
                    }),
                );
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Edge { path_to_bottom } => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, *path_to_bottom);
                let bottom_hash =
                    self.compute_filled_tree_rec::<H, TH>(bottom_node_index, output_map)?;
                let data = NodeData::Edge(EdgeData {
                    path_to_bottom: *path_to_bottom,
                    bottom_hash,
                });
                let hash_value = TH::compute_node_hash(&data);
                output_map.insert(
                    index,
                    Mutex::new(FilledNode {
                        hash: hash_value,
                        data,
                    }),
                );
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Sibling(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf(node_data) => {
                let hash_value = TH::compute_node_hash(&NodeData::Leaf(node_data.clone()));
                output_map.insert(
                    index,
                    Mutex::new(FilledNode {
                        hash: hash_value,
                        data: NodeData::Leaf(node_data.clone()),
                    }),
                );
                Ok(hash_value)
            }
        }
    }
}

impl<L: LeafDataTrait + std::clone::Clone> UpdatedSkeletonTree<L> for UpdatedSkeletonTreeImpl<L> {
    fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
    ) -> Result<impl FilledTree<L>, UpdatedSkeletonTreeError> {
        // 1. Create a new hashmap for the filled tree.
        let mut filled_tree_map = HashMap::new();
        // 2. Compute the filled tree hashmap from the skeleton_tree.
        self.compute_filled_tree_rec::<H, TH>(NodeIndex::root_index(), &mut filled_tree_map)?;
        // 3. Create a new FilledTreeImpl from the hashmap.
        Ok(FilledTreeImpl::new(filled_tree_map))
    }
}
