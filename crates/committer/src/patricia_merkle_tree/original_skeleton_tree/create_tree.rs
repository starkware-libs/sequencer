use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::BinaryData;
use crate::patricia_merkle_tree::node_data::inner_node::EdgeData;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::node_data::leaf::LeafModifications;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::SubTreeHeight;
use crate::patricia_merkle_tree::{
    original_skeleton_tree::node::OriginalSkeletonNode, types::NodeIndex,
};
use crate::storage::errors::StorageError;
use crate::storage::storage_trait::create_db_key;
use crate::storage::storage_trait::Storage;
use crate::storage::storage_trait::StorageKey;
use crate::storage::storage_trait::StoragePrefix;
use bisection::{bisect_left, bisect_right};
use std::collections::HashMap;
#[cfg(test)]
#[path = "create_tree_test.rs"]
pub mod create_tree_test;

#[derive(Debug, PartialEq)]
struct SubTree<'a> {
    pub sorted_leaf_indices: &'a [NodeIndex],
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self) -> SubTreeHeight {
        SubTreeHeight::new(SubTreeHeight::ACTUAL_HEIGHT.0 - (self.root_index.bit_length() - 1))
    }

    pub(crate) fn split_leaves(&self) -> [&'a [NodeIndex]; 2] {
        split_leaves(&self.root_index, self.sorted_leaf_indices)
    }

    pub(crate) fn is_unmodified(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    fn get_bottom_subtree(&self, path_to_bottom: &PathToBottom, bottom_hash: HashOutput) -> Self {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let bottom_leaves =
            &self.sorted_leaf_indices[bisect_left(self.sorted_leaf_indices, &leftmost_in_subtree)
                ..bisect_right(self.sorted_leaf_indices, &rightmost_in_subtree)];

        Self {
            sorted_leaf_indices: bottom_leaves,
            root_index: bottom_index,
            root_hash: bottom_hash,
        }
    }

    fn get_children_subtrees(&self, left_hash: HashOutput, right_hash: HashOutput) -> (Self, Self) {
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

    fn is_leaf(&self) -> bool {
        u8::from(self.get_height()) == 0
    }
}

impl OriginalSkeletonTreeImpl {
    /// Fetches the Patricia witnesses, required to build the original skeleton tree from storage.
    /// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
    /// unmodified nodes.
    fn fetch_nodes<L: LeafData>(
        &mut self,
        subtrees: Vec<SubTree<'_>>,
        storage: &impl Storage,
    ) -> OriginalSkeletonTreeResult<()> {
        if subtrees.is_empty() {
            return Ok(());
        }
        let mut next_subtrees = Vec::new();
        let filled_roots = Self::calculate_subtrees_roots::<L>(&subtrees, storage)?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(subtrees.iter()) {
            match filled_root.data {
                // Binary node.
                NodeData::Binary(BinaryData {
                    left_hash,
                    right_hash,
                }) => {
                    if subtree.is_unmodified() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                        continue;
                    }
                    self.nodes
                        .insert(subtree.root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_hash, right_hash);
                    next_subtrees.extend(vec![left_subtree, right_subtree]);
                }
                // Edge node.
                NodeData::Edge(EdgeData {
                    bottom_hash,
                    path_to_bottom,
                }) => {
                    self.nodes.insert(
                        subtree.root_index,
                        OriginalSkeletonNode::Edge(path_to_bottom),
                    );
                    if subtree.is_unmodified() {
                        self.nodes.insert(
                            path_to_bottom.bottom_index(subtree.root_index),
                            OriginalSkeletonNode::UnmodifiedSubTree(bottom_hash),
                        );
                        continue;
                    }
                    // Parse bottom.
                    let bottom_subtree = subtree.get_bottom_subtree(&path_to_bottom, bottom_hash);
                    next_subtrees.push(bottom_subtree);
                }
                // Leaf node.
                NodeData::Leaf(_previous_leaf) => {
                    if subtree.is_unmodified() {
                        // Sibling leaf.
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                    }
                }
            }
        }
        self.fetch_nodes::<L>(next_subtrees, storage)
    }

    fn calculate_subtrees_roots<L: LeafData>(
        subtrees: &[SubTree<'_>],
        storage: &impl Storage,
    ) -> OriginalSkeletonTreeResult<Vec<FilledNode<L>>> {
        let mut subtrees_roots = vec![];
        let db_keys: Vec<StorageKey> = subtrees
            .iter()
            .map(|subtree| {
                create_db_key(
                    if subtree.is_leaf() {
                        L::prefix()
                    } else {
                        StoragePrefix::InnerNode
                    },
                    &subtree.root_hash.0.to_bytes_be(),
                )
            })
            .collect();

        let db_vals = storage.mget(&db_keys);
        for ((subtree, optional_val), db_key) in
            subtrees.iter().zip(db_vals.iter()).zip(db_keys.into_iter())
        {
            let val = optional_val.ok_or(StorageError::MissingKey(db_key))?;
            subtrees_roots.push(FilledNode::deserialize(subtree.root_hash, val)?)
        }
        Ok(subtrees_roots)
    }

    pub(crate) fn create_impl<L: LeafData>(
        storage: &impl Storage,
        leaf_modifications: &LeafModifications<L>,
        root_hash: HashOutput,
    ) -> OriginalSkeletonTreeResult<Self> {
        let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
        sorted_leaf_indices.sort();
        if sorted_leaf_indices.is_empty() {
            return Ok(Self {
                nodes: HashMap::from([(
                    NodeIndex::ROOT,
                    OriginalSkeletonNode::UnmodifiedSubTree(root_hash),
                )]),
            });
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok(Self {
                nodes: HashMap::new(),
            });
        }
        let main_subtree = SubTree {
            sorted_leaf_indices: &sorted_leaf_indices,
            root_index: NodeIndex::ROOT,
            root_hash,
        };
        let mut skeleton_tree = Self {
            nodes: HashMap::new(),
        };
        skeleton_tree.fetch_nodes::<L>(vec![main_subtree], storage)?;
        Ok(skeleton_tree)
    }
}
