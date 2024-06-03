use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::BinaryData;
use crate::patricia_merkle_tree::node_data::inner_node::EdgeData;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonInputNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;
use crate::patricia_merkle_tree::types::TreeHeight;
use crate::patricia_merkle_tree::{
    original_skeleton_tree::node::OriginalSkeletonNode, types::NodeIndex,
};
use crate::storage::db_object::Deserializable;
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

struct SubTree<'a> {
    pub sorted_leaf_indices: &'a [NodeIndex],
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self) -> TreeHeight {
        TreeHeight::new(TreeHeight::MAX.0 - (self.root_index.bit_length() - 1))
    }

    pub(crate) fn split_leaves(&self) -> [&'a [NodeIndex]; 2] {
        split_leaves(&TreeHeight::MAX, &self.root_index, self.sorted_leaf_indices)
    }

    pub(crate) fn is_sibling(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    fn get_bottom_subtree(&self, path_to_bottom: &PathToBottom, bottom_hash: HashOutput) -> Self {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - TreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree + (NodeIndex::ROOT << bottom_height.into()) - NodeIndex::ROOT;
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
    /// Given a list of subtrees, traverses towards their leaves and fetches all non-empty and
    /// sibling nodes. Assumes no subtrees of height 0 (leaves).

    fn fetch_nodes(
        &mut self,
        subtrees: Vec<SubTree<'_>>,
        storage: &impl Storage,
    ) -> OriginalSkeletonTreeResult<()> {
        if subtrees.is_empty() {
            return Ok(());
        }
        let mut next_subtrees = Vec::new();
        let subtrees_roots = Self::calculate_subtrees_roots(&subtrees, storage)?;
        for (skeleton_node_input, subtree) in subtrees_roots.into_iter().zip(subtrees.iter()) {
            match skeleton_node_input {
                // Binary node.
                OriginalSkeletonInputNode::Binary {
                    hash,
                    data:
                        BinaryData {
                            left_hash,
                            right_hash,
                        },
                } => {
                    if subtree.is_sibling() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::LeafOrBinarySibling(hash),
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
                OriginalSkeletonInputNode::Edge(EdgeData {
                    bottom_hash,
                    path_to_bottom,
                }) => {
                    self.nodes.insert(
                        subtree.root_index,
                        OriginalSkeletonNode::Edge(path_to_bottom),
                    );
                    if subtree.is_sibling() {
                        self.nodes.insert(
                            path_to_bottom.bottom_index(subtree.root_index),
                            OriginalSkeletonNode::UnmodifiedBottom(bottom_hash),
                        );
                        continue;
                    }
                    // Parse bottom.
                    let bottom_subtree = subtree.get_bottom_subtree(&path_to_bottom, bottom_hash);
                    next_subtrees.push(bottom_subtree);
                }
                // Leaf node.
                OriginalSkeletonInputNode::Leaf(hash) => {
                    if subtree.is_sibling() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::LeafOrBinarySibling(hash),
                        );
                    }
                }
            }
        }
        self.fetch_nodes(next_subtrees, storage)
    }

    fn calculate_subtrees_roots(
        subtrees: &[SubTree<'_>],
        storage: &impl Storage,
    ) -> OriginalSkeletonTreeResult<Vec<OriginalSkeletonInputNode>> {
        let mut subtrees_roots = vec![];
        for subtree in subtrees.iter() {
            if subtree.is_leaf() {
                subtrees_roots.push(OriginalSkeletonInputNode::Leaf(subtree.root_hash));
                continue;
            }
            let key = create_db_key(StoragePrefix::InnerNode, &subtree.root_hash.0.to_bytes_be());
            let val = storage.get(&key).ok_or(StorageError::MissingKey(key))?;
            subtrees_roots.push(OriginalSkeletonInputNode::deserialize(
                &StorageKey::from(subtree.root_hash.0),
                val,
            )?)
        }
        Ok(subtrees_roots)
    }

    pub(crate) fn create_impl(
        storage: &impl Storage,
        sorted_leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
    ) -> OriginalSkeletonTreeResult<Self> {
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok(Self {
                nodes: HashMap::new(),
            });
        }
        let main_subtree = SubTree {
            sorted_leaf_indices,
            root_index: NodeIndex::ROOT,
            root_hash,
        };
        let mut skeleton_tree = Self {
            nodes: HashMap::new(),
        };
        skeleton_tree.fetch_nodes(vec![main_subtree], storage)?;
        Ok(skeleton_tree)
    }
}
