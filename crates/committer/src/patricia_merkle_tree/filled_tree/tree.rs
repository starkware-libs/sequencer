use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::{CompiledClassHash, FilledNode};
use crate::patricia_merkle_tree::node_data::errors::LeafError;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, Leaf, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;
use crate::storage::db_object::DBObject;
use crate::storage::storage_trait::{StorageKey, StorageValue};

#[cfg(test)]
#[path = "tree_test.rs"]
pub mod tree_test;

pub(crate) type FilledTreeResult<T, L> = Result<T, FilledTreeError<L>>;
/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub(crate) trait FilledTree<L: Leaf>: Sized + Send {
    /// Computes and returns the filled tree.
    async fn create<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'static>,
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, L::I>>,
    ) -> FilledTreeResult<(Self, Option<HashMap<NodeIndex, L::O>>), L>;

    async fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'static>,
        leaf_modifications: LeafModifications<L>,
    ) -> FilledTreeResult<Self, L>;

    // async fn create_no_leaf_output<'a, TH: TreeHashFunction<L> + 'static>(
    //     updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'static>,
    //     leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, L::I>>,
    // ) -> FilledTreeResult<Self, L> {
    //     let (result, _) = Self::create::<TH>(updated_skeleton, leaf_index_to_leaf_input).await?;
    //     Ok(result)
    // }

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

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

impl<L: Leaf + 'static> FilledTreeImpl<L> {
    fn initialize_filled_tree_map_with_placeholders<'a>(
        updated_skeleton: &Arc<impl UpdatedSkeletonTree<'a>>,
    ) -> HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>> {
        let mut filled_tree_map = HashMap::new();
        for (index, node) in updated_skeleton.get_nodes() {
            if !matches!(node, UpdatedSkeletonNode::UnmodifiedSubTree(_)) {
                filled_tree_map.insert(index, Mutex::new(None));
            }
        }
        filled_tree_map
    }

    fn initialize_leaf_output_map_with_placeholders(
        leaf_index_to_leaf_input: &Arc<HashMap<NodeIndex, L::I>>,
    ) -> Arc<HashMap<NodeIndex, Mutex<Option<L::O>>>> {
        Arc::new(leaf_index_to_leaf_input.keys().map(|index| (*index, Mutex::new(None))).collect())
    }

    pub(crate) fn get_all_nodes(&self) -> &HashMap<NodeIndex, FilledNode<L>> {
        &self.tree_map
    }

    /// Writes the hash and data to the output map. The writing is done in a thread-safe manner with
    /// interior mutability to avoid thread contention.
    fn write_to_output_map(
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
        index: NodeIndex,
        hash: HashOutput,
        data: NodeData<L>,
    ) -> FilledTreeResult<(), L> {
        match output_map.get(&index) {
            Some(node) => {
                let mut node = node
                    .lock()
                    .map_err(|_| FilledTreeError::PoisonedLock("Cannot lock node.".to_owned()))?;
                match node.take() {
                    Some(existing_node) => Err(FilledTreeError::DoubleOutputUpdate {
                        index,
                        existing_value: Box::new(existing_node),
                    }),
                    None => {
                        *node = Some(FilledNode { hash, data });
                        Ok(())
                    }
                }
            }
            None => Err(FilledTreeError::<L>::MissingNode(index)),
        }
    }

    /// Similar to `write_to_output_map`, but for the additional output map.
    //TODO(Amos, 1/8/2024): Panic makes more sense than returning an error here. Also - why not
    // use `write_to_output_map`?
    fn write_to_leaf_output_map(
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<L::O>>>>,
        index: NodeIndex,
        data: L::O,
    ) -> FilledTreeResult<(), L> {
        match output_map.get(&index) {
            Some(leaf) => {
                let mut leaf = leaf
                    .lock()
                    .map_err(|_| FilledTreeError::PoisonedLock("Cannot lock leaf.".to_owned()))?;
                match leaf.take() {
                    Some(existing_value) => {
                        Err(FilledTreeError::DoubleLeafOutputUpdate { index, existing_value })
                    }
                    None => {
                        *leaf = Some(data);
                        Ok(())
                    }
                }
            }
            None => Err(FilledTreeError::<L>::MissingNode(index)),
        }
    }

    fn remove_arc_mutex_and_option<V>(
        hash_map_in: Arc<HashMap<NodeIndex, Mutex<Option<V>>>>,
        fail_on_none_value: bool,
    ) -> FilledTreeResult<HashMap<NodeIndex, V>, L> {
        let mut hash_map_out = HashMap::new();
        for (key, value) in Arc::into_inner(hash_map_in)
            .unwrap_or_else(|| panic!("Cannot retrieve hashmap from Arc."))
        {
            let mut value = value
                .lock()
                .map_err(|_| FilledTreeError::<L>::PoisonedLock("Cannot lock node.".to_owned()))?;
            match value.take() {
                Some(value) => {
                    hash_map_out.insert(key, value);
                }
                None => {
                    if fail_on_none_value {
                        return Err(FilledTreeError::<L>::MissingNode(key));
                    }
                }
            }
        }
        Ok(hash_map_out)
    }

    fn wrap_leaf_input_keys_in_mutex(
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, L::I>>,
    ) -> Arc<HashMap<NodeIndex, Mutex<Option<L::I>>>> {
        // TODO(Amos, 1/8/2024): Can this be done without exiting the Arc?
        let res = Arc::into_inner(leaf_index_to_leaf_input)
            .expect("Cannot retrieve hashmap from Arc.")
            .into_iter()
            .map(|(k, v)| (k, Mutex::new(Some(v))))
            .collect();
        Arc::new(res)
    }

    #[async_recursion]
    async fn compute_filled_tree_rec<'a, TH>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'async_recursion + 'static>,
        index: NodeIndex,
        leaf_modifications: Arc<Option<LeafModifications<L>>>,
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, Mutex<Option<L::I>>>>,
        output_map: Arc<HashMap<NodeIndex, Mutex<Option<FilledNode<L>>>>>,
        leaf_index_to_leaf_output: Arc<HashMap<NodeIndex, Mutex<Option<L::O>>>>,
    ) -> FilledTreeResult<HashOutput, L>
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
                        Arc::clone(&leaf_index_to_leaf_input),
                        Arc::clone(&output_map),
                        Arc::clone(&leaf_index_to_leaf_output),
                    )),
                    tokio::spawn(Self::compute_filled_tree_rec::<TH>(
                        Arc::clone(&updated_skeleton),
                        right_index,
                        Arc::clone(&leaf_modifications),
                        Arc::clone(&leaf_index_to_leaf_input),
                        Arc::clone(&output_map),
                        Arc::clone(&leaf_index_to_leaf_output),
                    )),
                );

                let data = NodeData::Binary(BinaryData {
                    left_hash: left_hash.await??,
                    right_hash: right_hash.await??,
                });

                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(output_map, index, hash_value, data)?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::Edge(path_to_bottom) => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, path_to_bottom);
                let bottom_hash = Self::compute_filled_tree_rec::<TH>(
                    Arc::clone(&updated_skeleton),
                    bottom_node_index,
                    Arc::clone(&leaf_modifications),
                    Arc::clone(&leaf_index_to_leaf_input),
                    Arc::clone(&output_map),
                    Arc::clone(&leaf_index_to_leaf_output),
                )
                .await?;
                let data =
                    NodeData::Edge(EdgeData { path_to_bottom: *path_to_bottom, bottom_hash });
                let hash_value = TH::compute_node_hash(&data);
                Self::write_to_output_map(output_map, index, hash_value, data)?;
                Ok(hash_value)
            }
            UpdatedSkeletonNode::UnmodifiedSubTree(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf => {
                // TODO(Amos): Use correct error types, and consider returning error instead of
                // panic.
                let (leaf_data, leaf_output) = match Option::as_ref(&leaf_modifications) {
                    Some(leaf_modifications) => {
                        let leaf_data = leaf_modifications
                            .get(&index)
                            .ok_or(LeafError::MissingLeafModificationData(index))?
                            .clone();
                        (leaf_data, None)
                    }
                    None => {
                        let leaf_input = leaf_index_to_leaf_input
                            .get(&index)
                            .ok_or(LeafError::MissingLeafModificationData(index))?
                            .lock()
                            .map_err(|_| {
                                FilledTreeError::<L>::PoisonedLock("Cannot lock node.".to_owned())
                            })?
                            .take()
                            .unwrap_or_else(|| panic!("Missing input for leaf {:?}.", index));
                        L::create(leaf_input).await?
                    }
                };
                if leaf_data.is_empty() {
                    return Err(FilledTreeError::<L>::DeletedLeafInSkeleton(index));
                }
                let node_data = NodeData::Leaf(leaf_data);
                let hash_value = TH::compute_node_hash(&node_data);
                Self::write_to_output_map(output_map, index, hash_value, node_data)?;
                if let Some(output) = leaf_output {
                    Self::write_to_leaf_output_map(leaf_index_to_leaf_output, index, output)?
                };
                Ok(hash_value)
            }
        }
    }

    fn create_unmodified<'a>(
        updated_skeleton: &Arc<impl UpdatedSkeletonTree<'a>>,
    ) -> Result<Self, FilledTreeError<L>> {
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
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'static>,
        leaf_index_to_leaf_input: Arc<HashMap<NodeIndex, L::I>>,
    ) -> Result<(Self, Option<HashMap<NodeIndex, L::O>>), FilledTreeError<L>> {
        // Compute the filled tree in two steps:
        //   1. Create a map containing the tree structure without hash values.
        //   2. Fill in the hash values.
        if leaf_index_to_leaf_input.is_empty() {
            let unmodified = Self::create_unmodified(&updated_skeleton)?;
            return Ok((unmodified, Some(HashMap::new())));
        }

        if updated_skeleton.is_empty() {
            return Ok((Self::create_empty(), Some(HashMap::new())));
        }

        let filled_tree_map =
            Arc::new(Self::initialize_filled_tree_map_with_placeholders(&updated_skeleton));
        let leaf_index_to_leaf_output =
            Self::initialize_leaf_output_map_with_placeholders(&leaf_index_to_leaf_input);
        let wrapped_leaf_index_to_leaf_input =
            Self::wrap_leaf_input_keys_in_mutex(leaf_index_to_leaf_input);
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::clone(&updated_skeleton),
            NodeIndex::ROOT,
            Arc::new(None),
            Arc::clone(&wrapped_leaf_index_to_leaf_input),
            Arc::clone(&filled_tree_map),
            Arc::clone(&leaf_index_to_leaf_output),
        )
        .await?;
        let unwrapped_leaf_index_to_leaf_output =
            Self::remove_arc_mutex_and_option(leaf_index_to_leaf_output, false)?;

        Ok((
            FilledTreeImpl {
                tree_map: Self::remove_arc_mutex_and_option(filled_tree_map, true)?,
                root_hash,
            },
            Some(unwrapped_leaf_index_to_leaf_output),
        ))
    }

    async fn create_with_existing_leaves<'a, TH: TreeHashFunction<L> + 'static>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'static>,
        leaf_modifications: LeafModifications<L>,
    ) -> FilledTreeResult<Self, L> {
        // Compute the filled tree in two steps:
        //   1. Create a map containing the tree structure without hash values.
        //   2. Fill in the hash values.
        if leaf_modifications.is_empty() {
            return Self::create_unmodified(&updated_skeleton);
        }

        if updated_skeleton.is_empty() {
            return Ok(Self::create_empty());
        }

        let filled_tree_map =
            Arc::new(Self::initialize_filled_tree_map_with_placeholders(&updated_skeleton));
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::clone(&updated_skeleton),
            NodeIndex::ROOT,
            Arc::new(Some(leaf_modifications)),
            Arc::new(HashMap::new()),
            Arc::clone(&filled_tree_map),
            Arc::new(HashMap::new()),
        )
        .await?;

        Ok(FilledTreeImpl {
            tree_map: Self::remove_arc_mutex_and_option(filled_tree_map, true)?,
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
