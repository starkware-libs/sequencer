use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};

use async_recursion::async_recursion;
use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::DbHashMap;

use crate::db_layout::NodeLayoutFor;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::HashFilledNode;
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
    fn serialize<Layout: NodeLayoutFor<L>>(
        &self,
        key_context: &<Layout::DbLeaf as HasStaticPrefix>::KeyContext,
    ) -> SerializationResult<DbHashMap>;

    fn get_root_hash(&self) -> HashOutput;
}

#[derive(Debug, Eq, PartialEq)]
pub struct FilledTreeImpl<L>
where
    L: Leaf,
{
    pub tree_map: HashMap<NodeIndex, HashFilledNode<L>>,
    pub root_hash: HashOutput,
}

/// Determines how the leaves are obtained while filling the tree.
enum LeafSource<L: Leaf> {
    /// The leaves are already known and are read from these modifications.
    ExistingLeaves(LeafModifications<L>),
    /// The leaves are computed from their inputs, and the resulting `L::Output`s are written to
    /// `leaf_index_to_leaf_output`.
    ComputeLeaves {
        leaf_index_to_leaf_input: HashMap<NodeIndex, Mutex<Option<L::Input>>>,
        leaf_index_to_leaf_output: Arc<HashMap<NodeIndex, OnceLock<L::Output>>>,
    },
}

impl<L: Leaf + 'static> FilledTreeImpl<L> {
    fn initialize_filled_tree_output_map_with_placeholders<'a>(
        updated_skeleton: &impl UpdatedSkeletonTree<'a>,
    ) -> HashMap<NodeIndex, OnceLock<HashFilledNode<L>>> {
        let nodes_iter = updated_skeleton.get_nodes();
        let capacity = nodes_iter.size_hint().1.unwrap_or_default();
        let mut filled_tree_output_map = HashMap::with_capacity(capacity);
        for (index, node) in nodes_iter {
            if !matches!(node, UpdatedSkeletonNode::UnmodifiedSubTree(_)) {
                filled_tree_output_map.insert(index, OnceLock::new());
            }
        }
        filled_tree_output_map
    }

    fn initialize_leaf_output_map_with_placeholders(
        leaf_index_to_leaf_input: &HashMap<NodeIndex, L::Input>,
    ) -> HashMap<NodeIndex, OnceLock<L::Output>> {
        leaf_index_to_leaf_input.keys().map(|index| (*index, OnceLock::new())).collect()
    }

    pub(crate) fn get_all_nodes(&self) -> &HashMap<NodeIndex, HashFilledNode<L>> {
        &self.tree_map
    }

    /// Writes the hash and data to the output map. Each slot is written exactly once.
    fn write_to_output_map<T: Debug + Send + Sync>(
        output_map: &HashMap<NodeIndex, OnceLock<T>>,
        index: NodeIndex,
        output: T,
    ) -> FilledTreeResult<()> {
        match output_map.get(&index) {
            Some(slot) => slot.set(output).map_err(|_| {
                let existing_node = slot.get().expect("OnceLock is occupied after a failed set.");
                FilledTreeError::DoubleUpdate {
                    index,
                    existing_value_as_string: format!("{existing_node:?}"),
                }
            }),
            None => Err(FilledTreeError::MissingNodePlaceholder(index)),
        }
    }

    // Removes the `Arc` from the map and collects the value out of each `OnceLock` slot.
    fn collect_output_map<V>(
        output_map: Arc<HashMap<NodeIndex, OnceLock<V>>>,
        panic_if_empty_placeholder: bool,
    ) -> FilledTreeResult<HashMap<NodeIndex, V>> {
        let output_map = Arc::into_inner(output_map)
            .unwrap_or_else(|| panic!("Cannot retrieve output map from Arc."));

        let mut hash_map_out = HashMap::with_capacity(output_map.len());
        for (key, value) in output_map {
            match value.into_inner() {
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
    ) -> HashMap<NodeIndex, Mutex<Option<L::Input>>> {
        leaf_index_to_leaf_input.into_iter().map(|(k, v)| (k, Mutex::new(Some(v)))).collect()
    }

    // Computes the leaf at `index` from its corresponding leaf input, returning the leaf together
    // with its output.
    async fn compute_leaf(
        leaf_index_to_leaf_input: &HashMap<NodeIndex, Mutex<Option<L::Input>>>,
        index: NodeIndex,
    ) -> FilledTreeResult<(L, L::Output)> {
        let leaf_input = leaf_index_to_leaf_input
            .get(&index)
            .ok_or(FilledTreeError::MissingLeafInput(index))?
            .lock()
            .expect("Leaf input mutex does not expect to panic at locking the mutex")
            .take()
            .unwrap_or_else(|| panic!("Leaf input is None for index {index:?}."));
        L::create(leaf_input)
            .await
            .map_err(|leaf_error| FilledTreeError::Leaf { leaf_error, leaf_index: index })
    }

    // Recursively computes the filled tree. For `ComputeLeaves`, computes the leaves from the leaf
    // inputs and fills the leaf output map. For `ExistingLeaves`, retrieves the leaves from the
    // leaf modifications map.
    #[async_recursion]
    async fn compute_filled_tree_rec<'a, TH>(
        updated_skeleton: Arc<impl UpdatedSkeletonTree<'a> + 'async_recursion + 'static>,
        index: NodeIndex,
        leaf_source: Arc<LeafSource<L>>,
        filled_tree_output_map: Arc<HashMap<NodeIndex, OnceLock<HashFilledNode<L>>>>,
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
                        Arc::clone(&leaf_source),
                        Arc::clone(&filled_tree_output_map),
                    )),
                    tokio::spawn(Self::compute_filled_tree_rec::<TH>(
                        Arc::clone(&updated_skeleton),
                        right_index,
                        Arc::clone(&leaf_source),
                        Arc::clone(&filled_tree_output_map),
                    )),
                );

                let data = NodeData::Binary(BinaryData {
                    left_data: left_hash.await??,
                    right_data: right_hash.await??,
                });

                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    &filled_tree_output_map,
                    index,
                    HashFilledNode { hash, data },
                )?;
                Ok(hash)
            }
            UpdatedSkeletonNode::Edge(path_to_bottom) => {
                let bottom_node_index = NodeIndex::compute_bottom_index(index, path_to_bottom);
                let bottom_hash = Self::compute_filled_tree_rec::<TH>(
                    Arc::clone(&updated_skeleton),
                    bottom_node_index,
                    Arc::clone(&leaf_source),
                    Arc::clone(&filled_tree_output_map),
                )
                .await?;
                let data = NodeData::Edge(EdgeData {
                    path_to_bottom: *path_to_bottom,
                    bottom_data: bottom_hash,
                });
                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    &filled_tree_output_map,
                    index,
                    HashFilledNode { hash, data },
                )?;
                Ok(hash)
            }
            UpdatedSkeletonNode::UnmodifiedSubTree(hash_result) => Ok(*hash_result),
            UpdatedSkeletonNode::Leaf => {
                let leaf_data = match leaf_source.as_ref() {
                    LeafSource::ExistingLeaves(leaf_modifications) => {
                        L::from_modifications(&index, leaf_modifications).map_err(|leaf_error| {
                            FilledTreeError::Leaf { leaf_error, leaf_index: index }
                        })?
                    }
                    LeafSource::ComputeLeaves {
                        leaf_index_to_leaf_input,
                        leaf_index_to_leaf_output,
                    } => {
                        let (leaf_data, leaf_output) =
                            Self::compute_leaf(leaf_index_to_leaf_input, index).await?;
                        Self::write_to_output_map(leaf_index_to_leaf_output, index, leaf_output)?;
                        leaf_data
                    }
                };
                if leaf_data.is_empty() {
                    return Err(FilledTreeError::DeletedLeafInSkeleton(index));
                }
                let data = NodeData::Leaf(leaf_data);
                let hash = TH::compute_node_hash(&data);
                Self::write_to_output_map(
                    &filled_tree_output_map,
                    index,
                    HashFilledNode { hash, data },
                )?;
                Ok(hash)
            }
        }
    }

    fn create_unmodified<'a>(
        updated_skeleton: &impl UpdatedSkeletonTree<'a>,
    ) -> Result<Self, FilledTreeError> {
        let root_node = updated_skeleton.get_node(NodeIndex::ROOT)?;
        let UpdatedSkeletonNode::UnmodifiedSubTree(root_hash) = root_node else {
            panic!("A root of tree without modifications is expected to be an unmodified subtree.")
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

        // Wrap values in `OnceLock<T>` for write-once interior mutability.
        let filled_tree_output_map =
            Arc::new(Self::initialize_filled_tree_output_map_with_placeholders(&updated_skeleton));
        let leaf_index_to_leaf_output =
            Arc::new(Self::initialize_leaf_output_map_with_placeholders(&leaf_index_to_leaf_input));
        let leaf_source = Arc::new(LeafSource::ComputeLeaves {
            leaf_index_to_leaf_input: Self::wrap_leaf_inputs_for_interior_mutability(
                leaf_index_to_leaf_input,
            ),
            leaf_index_to_leaf_output: Arc::clone(&leaf_index_to_leaf_output),
        });

        // Compute the filled tree.
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::new(updated_skeleton),
            NodeIndex::ROOT,
            Arc::clone(&leaf_source),
            Arc::clone(&filled_tree_output_map),
        )
        .await?;

        // Drop the shared leaf source so `leaf_index_to_leaf_output` is uniquely held here and can
        // be reclaimed.
        drop(leaf_source);

        Ok((
            FilledTreeImpl {
                tree_map: Self::collect_output_map(filled_tree_output_map, true)?,
                root_hash,
            },
            Self::collect_output_map(leaf_index_to_leaf_output, false)?,
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

        // Wrap values in `OnceLock<T>` for write-once interior mutability.
        let filled_tree_output_map =
            Arc::new(Self::initialize_filled_tree_output_map_with_placeholders(&updated_skeleton));

        // Compute the filled tree.
        let root_hash = Self::compute_filled_tree_rec::<TH>(
            Arc::new(updated_skeleton),
            NodeIndex::ROOT,
            Arc::new(LeafSource::ExistingLeaves(leaf_modifications)),
            Arc::clone(&filled_tree_output_map),
        )
        .await?;

        Ok(FilledTreeImpl {
            tree_map: Self::collect_output_map(filled_tree_output_map, true)?,
            root_hash,
        })
    }

    fn serialize<Layout: NodeLayoutFor<L>>(
        &self,
        key_context: &<Layout::DbLeaf as HasStaticPrefix>::KeyContext,
    ) -> SerializationResult<DbHashMap> {
        // This function iterates over each node in the tree, using the node's `db_key` as the
        // hashmap key and the result of the node's `serialize` method as the value.
        self.get_all_nodes()
            .iter()
            .map(|(index, node)| {
                let (db_key, node_db_object) =
                    Layout::get_db_object(*index, key_context, node.clone());
                let db_value = node_db_object.serialize()?;
                Ok((db_key, db_value))
            })
            .collect::<Result<_, _>>()
    }

    fn get_root_hash(&self) -> HashOutput {
        self.root_hash
    }
}
