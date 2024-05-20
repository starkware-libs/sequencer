use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;

use crate::hash::hash_trait::{HashFunction, HashOutput};
use crate::patricia_merkle_tree::filled_tree::tree::FilledTree;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::{LeafData, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
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
pub(crate) trait UpdatedSkeletonTree<L: LeafData> {
    /// Computes and returns the filled tree.
    #[allow(dead_code)]
    async fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
        leaf_modifications: &LeafModifications<L>,
    ) -> Result<impl FilledTree<L>, UpdatedSkeletonTreeError<L>>;

    /// Does the skeleton represents an empty-tree (i.e. all leaves are empty).
    #[allow(dead_code)]
    fn is_empty(&self) -> bool;
}

pub(crate) struct UpdatedSkeletonTreeImpl {
    pub(crate) skeleton_tree: HashMap<NodeIndex, UpdatedSkeletonNode>,
}

impl UpdatedSkeletonTreeImpl {
    fn get_node<L: LeafData>(
        &self,
        index: NodeIndex,
    ) -> Result<&UpdatedSkeletonNode, UpdatedSkeletonTreeError<L>> {
        match self.skeleton_tree.get(&index) {
            Some(node) => Ok(node),
            None => Err(UpdatedSkeletonTreeError::MissingNode(index)),
        }
    }

    /// Writes the hash and data to the output map. The writing is done in a thread-safe manner with
    /// interior mutability to avoid thread contention.
    fn write_to_output_map<L: LeafData>(
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

    fn initialize_with_placeholders<L: LeafData>(
        &self,
    ) -> HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>> {
        let mut filled_tree_map = HashMap::new();
        for (index, node) in &self.skeleton_tree {
            if !matches!(node, UpdatedSkeletonNode::Sibling(_)) {
                filled_tree_map.insert(*index, Mutex::new(None));
            }
        }
        filled_tree_map
    }

    fn remove_arc_mutex_and_option<L: LeafData>(
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
    async fn compute_filled_tree_rec<H, TH, L>(
        &self,
        index: NodeIndex,
        leaf_modifications: &LeafModifications<L>,
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
    ) -> Result<HashOutput, UpdatedSkeletonTreeError<L>>
    where
        L: LeafData,
        H: HashFunction,
        TH: TreeHashFunction<L, H>,
    {
        let node = self.get_node(index)?;
        match node {
            UpdatedSkeletonNode::Binary => {
                let left_index = index * 2.into();
                let right_index = left_index + NodeIndex::ROOT;

                let (left_hash, right_hash) = tokio::join!(
                    self.compute_filled_tree_rec::<H, TH, L>(
                        left_index,
                        leaf_modifications,
                        Arc::clone(&output_map)
                    ),
                    self.compute_filled_tree_rec::<H, TH, L>(
                        right_index,
                        leaf_modifications,
                        Arc::clone(&output_map)
                    ),
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
                    .compute_filled_tree_rec::<H, TH, L>(
                        bottom_node_index,
                        leaf_modifications,
                        Arc::clone(&output_map),
                    )
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
            UpdatedSkeletonNode::Leaf(skeleton_leaf) => {
                let leaf_data = leaf_modifications
                    .get(&index)
                    .ok_or(UpdatedSkeletonTreeError::<L>::MissingDataForUpdate(index))?
                    .clone();
                if skeleton_leaf.is_empty() != leaf_data.is_empty() {
                    return Err(UpdatedSkeletonTreeError::<L>::InconsistentModification(
                        index,
                    ));
                }
                let node_data = NodeData::Leaf(leaf_data);
                let hash_value = TH::compute_node_hash(&node_data);
                Self::write_to_output_map(output_map, index, hash_value, node_data)?;
                Ok(hash_value)
            }
        }
    }
}

impl<L: LeafData + std::clone::Clone + std::marker::Sync + std::marker::Send> UpdatedSkeletonTree<L>
    for UpdatedSkeletonTreeImpl
{
    async fn compute_filled_tree<H: HashFunction, TH: TreeHashFunction<L, H>>(
        &self,
        leaf_modifications: &LeafModifications<L>,
    ) -> Result<FilledTreeImpl<L>, UpdatedSkeletonTreeError<L>> {
        // Compute the filled tree in two steps:
        //   1. Create a map containing the tree structure without hash values.
        //   2. Fill in the hash values.
        let filled_tree_map = Arc::new(self.initialize_with_placeholders());

        self.compute_filled_tree_rec::<H, TH, L>(
            NodeIndex::ROOT,
            leaf_modifications,
            Arc::clone(&filled_tree_map),
        )
        .await?;

        // Create and return a new FilledTreeImpl from the hashmap.
        Ok(FilledTreeImpl::new(Self::remove_arc_mutex_and_option(
            filled_tree_map,
        )?))
    }

    fn is_empty(&self) -> bool {
        todo!()
    }
}
