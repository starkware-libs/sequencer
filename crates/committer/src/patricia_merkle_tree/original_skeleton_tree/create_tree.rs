use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::BinaryData;
use crate::patricia_merkle_tree::node_data::inner_node::EdgeData;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
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
use log::warn;
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

    /// Returns the bottom subtree which is referred from `self` by the given path. When creating
    /// the bottom subtree some indices that were modified under `self` are not modified under the
    /// bottom subtree (leaves that were previously empty). These indices are returned as well.
    fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        bottom_hash: HashOutput,
    ) -> (Self, Vec<&NodeIndex>) {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let left_most_index = bisect_left(self.sorted_leaf_indices, &leftmost_in_subtree);
        let right_most_index = bisect_right(self.sorted_leaf_indices, &rightmost_in_subtree);
        let bottom_leaves = &self.sorted_leaf_indices[left_most_index..right_most_index];
        let previously_empty_leaf_indices = self.sorted_leaf_indices[..left_most_index]
            .iter()
            .chain(self.sorted_leaf_indices[right_most_index..].iter())
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
    /// unmodified nodes. If `compare_modified_leaves` is set, function logs out a warning when
    /// encountering a trivial modification. Fills the previous leaf values if it is not none.
    fn fetch_nodes<L: LeafData>(
        &mut self,
        subtrees: Vec<SubTree<'_>>,
        storage: &impl Storage,
        config: &impl OriginalSkeletonTreeConfig<L>,
        mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    ) -> OriginalSkeletonTreeResult<()> {
        if subtrees.is_empty() {
            return Ok(());
        }
        let should_fetch_leaves = config.compare_modified_leaves() || previous_leaves.is_some();
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

                    self.handle_subtree(&mut next_subtrees, left_subtree, should_fetch_leaves);
                    self.handle_subtree(&mut next_subtrees, right_subtree, should_fetch_leaves)
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
                    let (bottom_subtree, previously_empty_leaves_indices) =
                        subtree.get_bottom_subtree(&path_to_bottom, bottom_hash);
                    if let Some(ref mut leaves) = previous_leaves {
                        leaves.extend(
                            previously_empty_leaves_indices
                                .iter()
                                .map(|idx| (**idx, L::default()))
                                .collect::<HashMap<NodeIndex, L>>(),
                        );
                    }

                    if config.compare_modified_leaves() {
                        let empty_leaf = L::default();
                        for leaf_idx in previously_empty_leaves_indices {
                            if config.compare_leaf(leaf_idx, &empty_leaf)? {
                                warn!("Encountered a trivial modification at index {:?}, with value {:?}", leaf_idx, empty_leaf);
                            }
                        }
                    }

                    self.handle_subtree(&mut next_subtrees, bottom_subtree, should_fetch_leaves);
                }
                // Leaf node.
                NodeData::Leaf(previous_leaf) => {
                    if subtree.is_unmodified() {
                        // Sibling leaf.
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                    } else {
                        // Modified leaf.
                        if config.compare_modified_leaves()
                            && config.compare_leaf(&subtree.root_index, &previous_leaf)?
                        {
                            warn!(
                                "Encountered a trivial modification at index {:?}",
                                subtree.root_index
                            );
                        }
                        if let Some(ref mut leaves) = previous_leaves {
                            leaves.insert(subtree.root_index, previous_leaf);
                        }
                    }
                }
            }
        }
        self.fetch_nodes::<L>(next_subtrees, storage, config, previous_leaves)
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
        root_hash: HashOutput,
        sorted_leaf_indices: &[NodeIndex],
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<Self> {
        if sorted_leaf_indices.is_empty() {
            return Ok(Self::create_unmodified(root_hash));
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok(Self::create_empty());
        }
        let main_subtree = SubTree {
            sorted_leaf_indices,
            root_index: NodeIndex::ROOT,
            root_hash,
        };
        let mut skeleton_tree = Self {
            nodes: HashMap::new(),
        };
        skeleton_tree.fetch_nodes::<L>(vec![main_subtree], storage, config, None)?;
        Ok(skeleton_tree)
    }

    pub(crate) fn create_and_get_previous_leaves_impl<L: LeafData>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: &[NodeIndex],
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)> {
        if sorted_leaf_indices.is_empty() {
            let unmodified = Self::create_unmodified(root_hash);
            return Ok((unmodified, HashMap::new()));
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok((
                Self::create_empty(),
                sorted_leaf_indices
                    .iter()
                    .map(|idx| (*idx, L::default()))
                    .collect(),
            ));
        }
        let main_subtree = SubTree {
            sorted_leaf_indices,
            root_index: NodeIndex::ROOT,
            root_hash,
        };
        let mut skeleton_tree = Self {
            nodes: HashMap::new(),
        };
        let mut leaves = HashMap::new();
        skeleton_tree.fetch_nodes::<L>(vec![main_subtree], storage, config, Some(&mut leaves))?;
        Ok((skeleton_tree, leaves))
    }

    fn create_unmodified(root_hash: HashOutput) -> Self {
        Self {
            nodes: HashMap::from([(
                NodeIndex::ROOT,
                OriginalSkeletonNode::UnmodifiedSubTree(root_hash),
            )]),
        }
    }

    fn create_empty() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Handles a subtree referred by an edge or a binary node. Decides whether we deserialize the
    /// referred subtree or not.
    fn handle_subtree<'a>(
        &mut self,
        next_subtrees: &mut Vec<SubTree<'a>>,
        subtree: SubTree<'a>,
        should_fetch_leaves: bool,
    ) {
        if !subtree.is_leaf() || should_fetch_leaves {
            next_subtrees.push(subtree);
        } else if subtree.is_unmodified() {
            // Leaf sibling.
            self.nodes.insert(
                subtree.root_index,
                OriginalSkeletonNode::UnmodifiedSubTree(subtree.root_hash),
            );
        }
    }
}
