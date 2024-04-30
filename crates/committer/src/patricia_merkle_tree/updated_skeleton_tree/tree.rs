use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;

use crate::felt::Felt;
use crate::hash::hash_trait::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::filled_tree::tree::FilledTree;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::LeafDataTrait;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// This trait represents the structure of the subtree which was modified in the update.
/// It also contains the hashes of the Sibling nodes on the Merkle paths from the updated leaves
/// to the root.
pub(crate) trait UpdatedSkeletonTree<L: LeafDataTrait + std::clone::Clone> {
    /// Computes and returns the filled tree.
    async fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
    ) -> Result<impl FilledTree<L>, UpdatedSkeletonTreeError<L>>;
}

pub(crate) struct UpdatedSkeletonTreeImpl<L: LeafDataTrait + std::clone::Clone> {
    skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode<L>>,
}

impl<L: LeafDataTrait + std::clone::Clone + std::marker::Sync + std::marker::Send>
    UpdatedSkeletonTreeImpl<L>
{
    fn get_node(
        &self,
        index: NodeIndex,
    ) -> Result<&UpdatedSkeletonNode<L>, UpdatedSkeletonTreeError<L>> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode(index)),
        }
    }

    /// Writes the hash and data to the output map. The writing is done in a thread-safe manner with
    /// interior mutability to avoid thread contention.
    fn write_to_output_map(
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
        index: NodeIndex,
        hash: HashOutput,
        data: NodeData<L>,
    ) -> Result<(), UpdatedSkeletonTreeError<L>> {
        match output_map.get(&index) {
            Some(node) => {
                let mut node = node.lock().map_err(|_| {
                    UpdatedSkeletonTreeError::PoisonedLock("Cannot lock node.".to_owned())
                })?;
                match node.take() {
                    Some(existing_node) => Err(UpdatedSkeletonTreeError::DoubleUpdate {
                        index,
                        existing_value: Box::new(existing_node),
                    }),
                    None => {
                        *node = Some(FilledNode { hash, data });
                        Ok(())
                    }
                }
            }
            None => Err(UpdatedSkeletonTreeError::MissingNode(index)),
        }
    }

    fn initialize_with_placeholders(&self) -> HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>> {
        let mut filled_tree_map = HashMap::new();
        for (index, node) in &self.skeleton_tree {
            if !matches!(node, UpdatedSkeletonNode::Sibling(_)) {
                filled_tree_map.insert(*index, Mutex::new(None));
            }
        }
        filled_tree_map
    }

    fn remove_arc_mutex_and_option(
        hash_map_in: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
    ) -> Result<HashMap<NodeIndex, FilledNode<L>>, UpdatedSkeletonTreeError<L>> {
        let mut hash_map_out = HashMap::new();
        for (key, value) in hash_map_in.iter() {
            let mut value = value.lock().map_err(|_| {
                UpdatedSkeletonTreeError::PoisonedLock("Cannot lock node.".to_owned())
            })?;
            match value.take() {
                Some(value) => {
                    hash_map_out.insert(*key, value);
                }
                None => return Err(UpdatedSkeletonTreeError::MissingNode(*key)),
            }
        }
        Ok(hash_map_out)
    }

    #[async_recursion]
    async fn compute_filled_tree_rec<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
        index: NodeIndex,
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
    ) -> Result<HashOutput, UpdatedSkeletonTreeError<L>> {
        let node = self.get_node(index)?;
        match node {
            UpdatedSkeletonNode::Binary => {
                let left_index = NodeIndex(index.0 * Felt::TWO);
                let right_index = NodeIndex(left_index.0 + Felt::ONE);

                let (left_hash, right_hash) = tokio::join!(
                    self.compute_filled_tree_rec::<H, TH>(left_index, Arc::clone(&output_map)),
                    self.compute_filled_tree_rec::<H, TH>(right_index, Arc::clone(&output_map)),
                );

                let data = NodeData::Binary(BinaryData {
                    left_hash: left_hash?,
                    right_hash: right_hash?,
                });

                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(output_map, index, hash_value, data)?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Edge { path_to_bottom } => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, path_to_bottom);
                let bottom_hash = self
                    .compute_filled_tree_rec::<H, TH>(bottom_node_index, Arc::clone(&output_map))
                    .await?;
                let data = NodeData::Edge(EdgeData {
                    path_to_bottom: *path_to_bottom,
                    bottom_hash,
                });
                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(output_map, index, hash_value, data)?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Sibling(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf(node_data) => {
                let data = NodeData::Leaf(node_data.clone());
                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(output_map, index, hash_value, data)?;
                Ok(hash_value)
            }
        }
    }
}

impl<L: LeafDataTrait + std::clone::Clone + std::marker::Sync + std::marker::Send>
    UpdatedSkeletonTree<L> for UpdatedSkeletonTreeImpl<L>
{
    async fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
    ) -> Result<FilledTreeImpl<L>, UpdatedSkeletonTreeError<L>> {
        // Compute the filled tree in two steps:
        //   1. Create a map containing the tree structure without hash values.
        //   2. Fill in the hash values.
        let filled_tree_map = Arc::new(self.initialize_with_placeholders());

        self.compute_filled_tree_rec::<H, TH>(
            NodeIndex::root_index(),
            Arc::clone(&filled_tree_map),
        )
        .await?;

        // Create and return a new FilledTreeImpl from the hashmap.
        Ok(FilledTreeImpl::new(Self::remove_arc_mutex_and_option(
            filled_tree_map,
        )?))
    }
}
