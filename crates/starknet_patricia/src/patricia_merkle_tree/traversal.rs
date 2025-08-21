use std::collections::HashMap;

use starknet_patricia_storage::errors::{DeserializationError, StorageError};
use starknet_patricia_storage::storage_trait::{
    create_db_key,
    DbKey,
    PatriciaStorageError,
    Storage,
};
use thiserror::Error;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use crate::patricia_merkle_tree::node_data::inner_node::{
    NodeData,
    PathToBottom,
    Preimage,
    PreimageMap,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};

#[cfg(test)]
#[path = "traversal_test.rs"]
pub mod traversal_test;

#[derive(Debug, Error)]
pub enum TraversalError {
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    StorageRead(#[from] StorageError),
    #[error(transparent)]
    PatriciaStorage(#[from] PatriciaStorageError),
}

pub type TraversalResult<T> = Result<T, TraversalError>;

#[derive(Debug, PartialEq)]
pub(crate) struct SubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self) -> SubTreeHeight {
        SubTreeHeight::new(SubTreeHeight::ACTUAL_HEIGHT.0 - (self.root_index.bit_length() - 1))
    }

    pub(crate) fn split_leaves(&self) -> [SortedLeafIndices<'a>; 2] {
        split_leaves(&self.root_index, &self.sorted_leaf_indices)
    }

    pub(crate) fn is_unmodified(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    pub(crate) fn get_root_prefix<L: Leaf>(&self) -> PatriciaPrefix {
        if self.is_leaf() {
            PatriciaPrefix::Leaf(L::get_static_prefix())
        } else {
            PatriciaPrefix::InnerNode
        }
    }

    /// Returns the bottom subtree which is referred from `self` by the given path. When creating
    /// the bottom subtree some indices that were modified under `self` are not modified under the
    /// bottom subtree (leaves that were previously empty). These indices are returned as well.
    pub(crate) fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        bottom_hash: HashOutput,
    ) -> (Self, Vec<&NodeIndex>) {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let leftmost_index = self.sorted_leaf_indices.bisect_left(&leftmost_in_subtree);
        let rightmost_index = self.sorted_leaf_indices.bisect_right(&rightmost_in_subtree);
        let bottom_leaves = self.sorted_leaf_indices.subslice(leftmost_index, rightmost_index);
        let previously_empty_leaf_indices = self.sorted_leaf_indices.get_indices()
            [..leftmost_index]
            .iter()
            .chain(self.sorted_leaf_indices.get_indices()[rightmost_index..].iter())
            .collect();

        (
            Self {
                sorted_leaf_indices: bottom_leaves,
                root_index: bottom_index,
                root_hash: bottom_hash,
            },
            previously_empty_leaf_indices,
        )
    }

    pub(crate) fn get_children_subtrees(
        &self,
        left_hash: HashOutput,
        right_hash: HashOutput,
    ) -> (Self, Self) {
        let [left_leaves, right_leaves] = self.split_leaves();
        let left_root_index = self.root_index * 2.into();
        (
            SubTree {
                sorted_leaf_indices: left_leaves,
                root_index: left_root_index,
                root_hash: left_hash,
            },
            SubTree {
                sorted_leaf_indices: right_leaves,
                root_index: left_root_index + NodeIndex::ROOT,
                root_hash: right_hash,
            },
        )
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.root_index.is_leaf()
    }
}

// TODO(Aviv, 17/07/2024): Split between storage prefix implementation and function logic.
pub(crate) fn calculate_subtrees_roots<'a, L: Leaf>(
    subtrees: &[SubTree<'a>],
    storage: &impl Storage,
) -> TraversalResult<Vec<FilledNode<L>>> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> = subtrees
        .iter()
        .map(|subtree| {
            create_db_key(subtree.get_root_prefix::<L>().into(), &subtree.root_hash.0.to_bytes_be())
        })
        .collect();

    let db_vals = storage.mget(&db_keys)?;
    for ((subtree, optional_val), db_key) in
        subtrees.iter().zip(db_vals.iter()).zip(db_keys.into_iter())
    {
        let Some(val) = optional_val else { Err(StorageError::MissingKey(db_key))? };
        subtrees_roots.push(FilledNode::deserialize(subtree.root_hash, val, subtree.is_leaf())?)
    }
    Ok(subtrees_roots)
}

/// Returns the Patricia inner nodes (`PreimageMap`) in the paths to the given `leaf_indices` in the
/// given tree according to the `root_hash`.
/// If `leaves` is not `None`, it also fetches the modified leaves.
pub fn fetch_patricia_paths<L: Leaf>(
    storage: &impl Storage,
    root_hash: HashOutput,
    leaf_indices: &mut [NodeIndex],
    leaves: Option<&mut HashMap<NodeIndex, L>>,
) -> TraversalResult<PreimageMap> {
    let mut witnesses = PreimageMap::new();

    if leaf_indices.is_empty() {
        return Ok(witnesses);
    }

    let main_subtree = SubTree {
        sorted_leaf_indices: SortedLeafIndices::new(leaf_indices),
        root_index: NodeIndex::ROOT,
        root_hash,
    };

    fetch_patricia_paths_inner::<L>(storage, vec![main_subtree], &mut witnesses, leaves)?;
    Ok(witnesses)
}

/// Fetches the inner nodes, required to `patricia_update`.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// inner nodes in their paths.
/// If `leaves` is not `None`, it also fetches the modified leaves.
fn fetch_patricia_paths_inner<'a, L: Leaf>(
    storage: &impl Storage,
    subtrees: Vec<SubTree<'a>>,
    witnesses: &mut PreimageMap,
    mut leaves: Option<&mut HashMap<NodeIndex, L>>,
) -> TraversalResult<()> {
    if subtrees.is_empty() {
        return Ok(());
    }
    let mut next_subtrees = Vec::new();
    let filled_roots = calculate_subtrees_roots::<L>(&subtrees, storage)?;
    for (filled_root, subtree) in filled_roots.into_iter().zip(subtrees.iter()) {
        // Always insert root.
        // No need to insert an unmodified node (which is not the root), because its parent is
        // inserted, and contains the preimage.
        if subtree.root_index != NodeIndex::ROOT && subtree.is_unmodified() {
            continue;
        }
        match filled_root.data {
            // Binary node.
            NodeData::Binary(binary_data) => {
                witnesses.insert(subtree.root_hash, Preimage::Binary(binary_data.clone()));
                let (left_subtree, right_subtree) =
                    subtree.get_children_subtrees(binary_data.left_hash, binary_data.right_hash);
                next_subtrees.push(left_subtree);
                next_subtrees.push(right_subtree);
            }
            // Edge node.
            NodeData::Edge(edge_data) => {
                witnesses.insert(subtree.root_hash, Preimage::Edge(edge_data));
                // Parse bottom.
                let (bottom_subtree, previously_empty_leaves_indices) =
                    subtree.get_bottom_subtree(&edge_data.path_to_bottom, edge_data.bottom_hash);
                if let Some(ref mut leaves_map) = leaves {
                    // Insert previously empty leaves as empty leaves.
                    for index in previously_empty_leaves_indices {
                        leaves_map.insert(*index, L::default());
                    }
                }
                next_subtrees.push(bottom_subtree);
            }
            // Leaf node.
            NodeData::Leaf(leaf_data) => {
                // Fetch the leaf if it's modified and should be fetched.
                if let Some(ref mut leaves_map) = leaves {
                    leaves_map.insert(subtree.root_index, leaf_data);
                }
            }
        }
    }
    fetch_patricia_paths_inner::<L>(storage, next_subtrees, witnesses, leaves)
}
