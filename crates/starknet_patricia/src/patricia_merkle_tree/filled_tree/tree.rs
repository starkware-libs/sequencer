use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;
use starknet_patricia_storage::db_object::DBObject;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

pub(crate) type FilledTreeResult<T> = Result<T, FilledTreeError>;
/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub trait FilledTree<L: Leaf>: Sized + Send {
    /// Computes and returns the filled tree and the leaf output map.
    fn create<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_index_to_leaf_input: HashMap<NodeIndex, L::Input>,
    ) -> impl Future<Output = FilledTreeResult<(Self, HashMap<NodeIndex, L::Output>)>> + Send;

    /// Computes and returns the filled tree using the provided leaf modifications. Since the
    /// leaves are not computed, no leaf output will be returned.
    fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_modifications: LeafModifications<L>,
    ) -> impl Future<Output = FilledTreeResult<Self>> + Send;

    /// Serializes the current state of the tree into a hashmap,
    /// where each key-value pair corresponds
    /// to a storage key and its serialized storage value.
    fn serialize(&self) -> HashMap<DbKey, DbValue>;

    fn get_root_hash(&self) -> HashOutput;
}

#[derive(Debug, Eq, PartialEq)]
pub struct FilledTreeImpl<L: Leaf> {
    pub tree_map: HashMap<NodeIndex, FilledNode<L>>,
    pub root_hash: HashOutput,
}

impl<L: Leaf + 'static> FilledTreeImpl<L> {
    fn initialize_filled_tree_output_map_with_placeholders<'a>(
        updated_skeleton: &impl UpdatedSkeletonTree<'a>,
    ) -> HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>> {
        let mut filled_tree_output_map = HashMap::new();
        for (index, node) in updated_skeleton.get_nodes() {
            if !matches!(node, UpdatedSkeletonNode::UnmodifiedSubTree(_)) {
                filled_tree_output_map.insert(index, Mutex::new(None));
            }
        }
        filled_tree_output_map
    }

    fn initialize_leaf_output_map_with_placeholders(
        leaf_index_to_leaf_input: &HashMap<NodeIndex, L::Input>,
    ) -> Arc<HashMap<NodeIndex, Mutex<Option<L::Output>>>> {
        Arc::new(leaf_index_to_leaf_input.keys().map(|index| (*index, Mutex::new(None))).collect())
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
                        existing_value_as_string: format!("{existing_node:?}"),
                    }),
                    None => {
                        *node = Some(output);
                        Ok(())
                    }
                }
            }
            None => Err(FilledTreeError::MissingNodePlaceholder(index)),
        }
    }

    // Removes the `Arc` from the map and unwraps the `Mutex` and `Option` from the value.
    // If `panic_if_empty_placeholder` is `true`, will panic if an empty placeholder is found.
    fn remove_arc_mutex_and_option_from_output_map<V>(
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<V>>>>,
        panic_if_empty_placeholder: bool,
    ) -> FilledTreeResult<HashMap<NodeIndex, V>> {
        let mut hash_map_out = HashMap::new();
        for (key, value) in Arc::into_inner(output_map)
            .unwrap_or_else(|| panic!("Cannot retrieve output map from Arc."))
        {
            let mut value = value
                .lock()
                .map_err(|_| FilledTreeError::PoisonedLock("Cannot lock node.".to_owned()))?;
            match value.take() {
                Some(unwrapped_value) => {
                    hash_map_out.insert(key, unwrapped_value);
                }
                None => {
                    if panic_if_empty_placeholder {
                        panic!("Empty placeholder in an output map for index {key:?}.");
                    }
                }
            }
        }
        Ok(hash_map_out)
    }

    fn wrap_leaf_inputs_for_interior_mutability(
        leaf_index_to_leaf_input: HashMap<NodeIndex, L::Input>,
    ) -> Arc<HashMap<NodeIndex, Mutex<Option<L::Input>>>> {
        Arc::new(
            leaf_index_to_leaf_input.into_iter().map(|(k, v)| (k, Mutex::new(Some(v)))).collect(),
        )
    }

    // If leaf modifications are `None`, will compute the leaf from the corresponding leaf input
    // and return the leaf output. Otherwise, will retrieve the leaf from the leaf modifications
    // and return `None` in place of the leaf output (ignoring the leaf input).
    async fn get_or_compute_leaf(
        leaf_modifications: Option<Arc<LeafModifications<L>>>,
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, Mutex<Option<L::Input>>>>,
        index: NodeIndex,
    ) -> FilledTreeResult<(L, Option<L::Output>)> {
        match leaf_modifications {
            Some(leaf_modifications) => {
                let leaf_data =
                    L::from_modifications(&index, &leaf_modifications).map_err(|leaf_err| {
                        FilledTreeError::Leaf { leaf_error: leaf_err, leaf_index: index }
                    })?;
                Ok((leaf_data, None))
            }
            None => {
                let leaf_input = leaf_index_to_leaf_input
                    .get(&index)
                    .ok_or(FilledTreeError::MissingLeafInput(index))?
                    .lock()
                    .map_err(|_| FilledTreeError::PoisonedLock("Cannot lock node.".to_owned()))?
                    .take()
                    .unwrap_or_else(|| panic!("Leaf input is None for index {index:?}."));
                let (leaf_data, leaf_output) = L::create(leaf_input).await.map_err(|leaf_err| {
                    FilledTreeError::Leaf { leaf_error: leaf_err, leaf_index: index }
                })?;
                Ok((leaf_data, Some(leaf_output)))
            }
        }
    }

    // Recursively computes the filled tree. If leaf modifications are `None`, will compute the
    // leaves from the leaf inputs and fill the leaf output map. Otherwise, will retrieve the
    // leaves from the leaf modifications map and ignore the input and output maps.
    #[async_recursion]
    async fn compute_filled_tree_rec<'a, TH>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'async_recursion + 'static>,
        index: NodeIndex,
        leaf_modifications: Option<Arc<LeafModifications<L>>>,
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, Mutex<Option<L::Input>>>>,
        filled_tree_output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
        leaf_index_to_leaf_output: Arc<HashMap<NodeIndex, Mutex<Option<L::Output>>>>,
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
                        leaf_modifications.as_ref().map(Arc::clone),
                        Arc::clone(&leaf_index_to_leaf_input),
                        Arc::clone(&filled_tree_output_map),
                        Arc::clone(&leaf_index_to_leaf_output),
                    )),
                    tokio::spawn(Self::compute_filled_tree_rec::<TH>(
                        Arc::clone(&updated_skeleton),
                        right_index,
                        leaf_modifications.as_ref().map(Arc::clone),
                        Arc::clone(&leaf_index_to_leaf_input),
                        Arc::clone(&filled_tree_output_map),
                        Arc::clone(&leaf_index_to_leaf_output),
                    )),
                );

                let data = NodeData::Binary(BinaryData {
                    left_hash: left_hash.await??,
                    right_hash: right_hash.await??,
                });

                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    filled_tree_output_map,
                    index,
                    FilledNode { hash, data },
                )?;
                Ok(hash)
            }
            UpdatedSkeletonNode::Edge(path_to_bottom) => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, path_to_bottom);
                let bottom_hash = Self::compute_filled_tree_rec::<TH>(
                    Arc::clone(&updated_skeleton),
                    bottom_node_index,
                    leaf_modifications.as_ref().map(Arc::clone),
                    Arc::clone(&leaf_index_to_leaf_input),
                    Arc::clone(&filled_tree_output_map),
                    Arc::clone(&leaf_index_to_leaf_output),
                )
                .await?;
                let data =
                    NodeData::Edge(EdgeData { path_to_bottom: *path_to_bottom, bottom_hash });
                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    filled_tree_output_map,
                    index,
                    FilledNode { hash, data },
                )?;
                Ok(hash)
            }
            UpdatedSkeletonNode::UnmodifiedSubTree(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf => {
                let (leaf_data, leaf_output) =
                    Self::get_or_compute_leaf(leaf_modifications, leaf_index_to_leaf_input, index)
                        .await?;
                if leaf_data.is_empty() {
                    return Err(FilledTreeError::DeletedLeafInSkeleton(index));
                }
                let data = NodeData::Leaf(leaf_data);
                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    filled_tree_output_map,
                    index,
                    FilledNode { hash, data },
                )?;
                if let Some(output) = leaf_output {
                    Self::write_to_output_map(leaf_index_to_leaf_output, index, output)?
                };
                Ok(hash)
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
    async fn create<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_index_to_leaf_input: HashMap<NodeIndex, L::Input>,
    ) -> Result<(Self, HashMap<NodeIndex, L::Output>), FilledTreeError> {
        // Handle edge cases of no leaf modifications.
        if leaf_index_to_leaf_input.is_empty() {
            let unmodified = Self::create_unmodified(&updated_skeleton)?;
            return Ok((unmodified, HashMap::new()));
        }
        if updated_skeleton.is_empty() {
            return Ok((Self::create_empty(), HashMap::new()));
        }

        // Wrap values in `Mutex<Option<T>>` for interior mutability.
        let filled_tree_output_map =
            Arc::new(Self::initialize_filled_tree_output_map_with_placeholders(&updated_skeleton));
        let leaf_index_to_leaf_output =
            Self::initialize_leaf_output_map_with_placeholders(&leaf_index_to_leaf_input);
        let wrapped_leaf_index_to_leaf_input =
            Self::wrap_leaf_inputs_for_interior_mutability(leaf_index_to_leaf_input);

        // Compute the filled tree.
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::new(updated_skeleton),
            NodeIndex::ROOT,
            None,
            Arc::clone(&wrapped_leaf_index_to_leaf_input),
            Arc::clone(&filled_tree_output_map),
            Arc::clone(&leaf_index_to_leaf_output),
        )
        .await?;

        Ok((
            FilledTreeImpl {
                tree_map: Self::remove_arc_mutex_and_option_from_output_map(
                    filled_tree_output_map,
                    true,
                )?,
                root_hash,
            },
            Self::remove_arc_mutex_and_option_from_output_map(leaf_index_to_leaf_output, false)?,
        ))
    }

    async fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: impl UpdatedSkeletonTree<'a> + 'static,
        leaf_modifications: LeafModifications<L>,
    ) -> FilledTreeResult<Self> {
        // Handle edge case of no modifications.
        if leaf_modifications.is_empty() {
            return Self::create_unmodified(&updated_skeleton);
        }
        if updated_skeleton.is_empty() {
            return Ok(Self::create_empty());
        }

        // Wrap values in `Mutex<Option<T>>`` for interior mutability.
        let filled_tree_output_map =
            Arc::new(Self::initialize_filled_tree_output_map_with_placeholders(&updated_skeleton));

        // Compute the filled tree.
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::new(updated_skeleton),
            NodeIndex::ROOT,
            Some(leaf_modifications.into()),
            Arc::new(HashMap::new()),
            Arc::clone(&filled_tree_output_map),
            Arc::new(HashMap::new()),
        )
        .await?;

        Ok(FilledTreeImpl {
            tree_map: Self::remove_arc_mutex_and_option_from_output_map(
                filled_tree_output_map,
                true,
            )?,
            root_hash,
        })
    }

    fn serialize(&self) -> HashMap<DbKey, DbValue> {
        // This function iterates over each node in the tree, using the node's `db_key` as the
        // hashmap key and the result of the node's `serialize` method as the value.
        self.get_all_nodes().values().map(|node| (node.db_key(), node.serialize())).collect()
    }

    fn get_root_hash(&self) -> HashOutput {
        self.root_hash
    }
}
