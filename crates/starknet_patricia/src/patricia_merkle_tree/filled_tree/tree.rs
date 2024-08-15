use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;
use crate::storage::db_object::DBObject;
use crate::storage::storage_trait::{StorageKey, StorageValue};

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

pub(crate) type FilledTreeResult<T> = Result<T, FilledTreeError>;
/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub trait FilledTree<L: Leaf>: Sized {
    /// Computes and returns the filled tree.
    fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_modifications: LeafModifications<L>,
    ) -> impl Future<Output = FilledTreeResult<Self>> + Send;

    /// Serializes the current state of the tree into a hashmap,
    /// where each key-value pair corresponds
    /// to a storage key and its serialized storage value.
    fn serialize(&self) -> HashMap<StorageKey, StorageValue>;

    fn get_root_hash(&self) -> HashOutput;
}

#[derive(Debug, Eq, PartialEq)]
pub struct FilledTreeImpl<L: Leaf> {
    pub tree_map: HashMap<NodeIndex, FilledNode<L>>,
    pub root_hash: HashOutput,
}

impl<L: Leaf + 'static> FilledTreeImpl<L> {
    fn initialize_filled_tree_map_with_placeholders<'a>(
        updated_skeleton: &impl UpdatedSkeletonTree<'a>,
    ) -> HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>> {
        let mut filled_tree_map = HashMap::new();
        for (index, node) in updated_skeleton.get_nodes() {
            if !matches!(node, UpdatedSkeletonNode::UnmodifiedSubTree(_)) {
                filled_tree_map.insert(index, Mutex::new(None));
            }
        }
        filled_tree_map
    }

    pub(crate) fn get_all_nodes(&self) -> &HashMap<NodeIndex, FilledNode<L>> {
        &self.tree_map
    }

    /// Writes the hash and data to the output map. The writing is done in a thread-safe manner with
    /// interior mutability to avoid thread contention.
    fn write_to_output_map<T: Debug>(
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<T>>>>,
        index: NodeIndex,
        output: T,
    ) -> FilledTreeResult<()> {
        match output_map.get(&index) {
            Some(node) => {
                let mut node = node.lock().map_err(|_| {
                    FilledTreeError::PoisonedLock("Cannot lock node in output map.".to_owned())
                })?;
                match node.take() {
                    Some(existing_node) => Err(FilledTreeError::DoubleUpdate {
                        index,
                        existing_value_as_string: format!("{:?}", existing_node),
                    }),
                    None => {
                        *node = Some(output);
                        Ok(())
                    }
                }
            }
            None => Err(FilledTreeError::MissingNode(index)),
        }
    }

    fn remove_arc_mutex_and_option(
        hash_map_in: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
    ) -> FilledTreeResult<HashMap<NodeIndex, FilledNode<L>>> {
        let mut hash_map_out = HashMap::new();
        for (key, value) in hash_map_in.iter() {
            let mut value = value
                .lock()
                .map_err(|_| FilledTreeError::PoisonedLock("Cannot lock node.".to_owned()))?;
            match value.take() {
                Some(value) => {
                    hash_map_out.insert(*key, value);
                }
                None => return Err(FilledTreeError::MissingNode(*key)),
            }
        }
        Ok(hash_map_out)
    }

    #[async_recursion]
    async fn compute_filled_tree_rec<'a, TH>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'async_recursion + 'static>,
        index: NodeIndex,
        leaf_modifications: Arc<LeafModifications<L>>,
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
    ) -> FilledTreeResult<HashOutput>
    where
        TH: TreeHashFunction<L> + 'static,
    {
        let node = updated_skeleton.get_node(index)?;
        match node {
            UpdatedSkeletonNode::Binary => {
                let left_index = index * 2.into();
                let right_index = left_index + NodeIndex::ROOT;

                let (left_hash, right_hash) = (
                    tokio::spawn(Self::compute_filled_tree_rec::<TH>(
                        Arc::clone(&updated_skeleton),
                        left_index,
                        Arc::clone(&leaf_modifications),
                        Arc::clone(&output_map),
                    )),
                    tokio::spawn(Self::compute_filled_tree_rec::<TH>(
                        Arc::clone(&updated_skeleton),
                        right_index,
                        Arc::clone(&leaf_modifications),
                        Arc::clone(&output_map),
                    )),
                );

                let data = NodeData::Binary(BinaryData {
                    left_hash: left_hash.await??,
                    right_hash: right_hash.await??,
                });

                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    output_map,
                    index,
                    FilledNode { hash: hash_value, data },
                )?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Edge(path_to_bottom) => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, path_to_bottom);
                let bottom_hash = Self::compute_filled_tree_rec::<TH>(
                    Arc::clone(&updated_skeleton),
                    bottom_node_index,
                    leaf_modifications,
                    Arc::clone(&output_map),
                )
                .await?;
                let data =
                    NodeData::Edge(EdgeData { path_to_bottom: *path_to_bottom, bottom_hash });
                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    output_map,
                    index,
                    FilledNode { hash: hash_value, data },
                )?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::UnmodifiedSubTree(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf => {
                // TODO(Amos): use `L::create` when no leaf modifications are provided.
                let leaf_data = L::from_modifications(&index, Arc::clone(&leaf_modifications))
                    .map_err(|e| FilledTreeError::Leaf { leaf_error: e, leaf_index: index })?;
                if leaf_data.is_empty() {
                    return Err(FilledTreeError::DeletedLeafInSkeleton(index));
                }
                let node_data = NodeData::Leaf(leaf_data);
                let hash_value = TH::compute_node_hash(&node_data);
                Self::write_to_output_map(
                    output_map,
                    index,
                    FilledNode { hash: hash_value, data: node_data },
                )?;
                Ok(hash_value)
            }
        }
    }

    fn create_unmodified<'a>(
        updated_skeleton: &impl UpdatedSkeletonTree<'a>,
    ) -> Result<Self, FilledTreeError> {
        let root_node = updated_skeleton.get_node(NodeIndex::ROOT)?;
        let UpdatedSkeletonNode::UnmodifiedSubTree(root_hash) = root_node else {
            panic!("A root of tree without modifications is expected to be a unmodified subtree.")
        };
        Ok(Self { tree_map: HashMap::new(), root_hash: *root_hash })
    }

    fn create_empty() -> Self {
        Self { tree_map: HashMap::new(), root_hash: HashOutput::ROOT_OF_EMPTY_TREE }
    }
}

impl<L: Leaf + 'static> FilledTree<L> for FilledTreeImpl<L> {
    async fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_modifications: LeafModifications<L>,
    ) -> FilledTreeResult<Self> {
        if leaf_modifications.is_empty() {
            return Self::create_unmodified(&updated_skeleton);
        }

        if updated_skeleton.is_empty() {
            return Ok(Self::create_empty());
        }

        // Wrap values in `Mutex<Option<T>>`` for interior mutability.
        let filled_tree_map =
            Arc::new(Self::initialize_filled_tree_map_with_placeholders(&updated_skeleton));

        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::new(updated_skeleton),
            NodeIndex::ROOT,
            Arc::new(leaf_modifications),
            Arc::clone(&filled_tree_map),
        )
        .await?;

        Ok(FilledTreeImpl {
            tree_map: Self::remove_arc_mutex_and_option(filled_tree_map)?,
            root_hash,
        })
    }

    fn serialize(&self) -> HashMap<StorageKey, StorageValue> {
        // This function iterates over each node in the tree, using the node's `db_key` as the
        // hashmap key and the result of the node's `serialize` method as the value.
        self.get_all_nodes().iter().map(|(_, node)| (node.db_key(), node.serialize())).collect()
    }

    fn get_root_hash(&self) -> HashOutput {
        self.root_hash
    }
}
