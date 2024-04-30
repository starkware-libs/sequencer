use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::inner_node::BinaryData;
use crate::patricia_merkle_tree::node_data::inner_node::EdgeData;
use crate::patricia_merkle_tree::node_data::inner_node::NodeData;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::TreeHeight;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::patricia_merkle_tree::{
    original_skeleton_tree::node::OriginalSkeletonNode, types::NodeIndex,
};
use crate::storage::errors::StorageError;
use crate::storage::serde_trait::Deserializable;
use crate::storage::storage_trait::create_db_key;
use crate::storage::storage_trait::Storage;
use crate::storage::storage_trait::StorageKey;
use crate::storage::storage_trait::StoragePrefix;
use bisection::{bisect_left, bisect_right};
use std::collections::HashMap;
#[cfg(test)]
#[path = "original_skeleton_calc_test.rs"]
pub mod original_skeleton_calc_test;

#[allow(dead_code)]
pub(crate) struct OriginalSkeletonTreeImpl {
    pub(crate) nodes: HashMap<NodeIndex, OriginalSkeletonNode<LeafData>>,
    tree_height: TreeHeight,
}

struct SubTree<'a> {
    pub sorted_leaf_indices: &'a [NodeIndex],
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self, total_tree_height: &TreeHeight) -> TreeHeight {
        TreeHeight(total_tree_height.0 - (self.root_index.0.bits() - 1))
    }

    pub(crate) fn split_leaves(
        &self,
        total_tree_height: &TreeHeight,
    ) -> (&'a [NodeIndex], &'a [NodeIndex]) {
        let height = self.get_height(total_tree_height);
        let leftmost_index_in_right_subtree = ((self.root_index.times_two_to_the_power(1))
            + NodeIndex(Felt::ONE))
        .times_two_to_the_power(height.0 - 1);
        let mid = bisect_left(self.sorted_leaf_indices, &leftmost_index_in_right_subtree);
        (
            &self.sorted_leaf_indices[..mid],
            &self.sorted_leaf_indices[mid..],
        )
    }

    pub(crate) fn is_sibling(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        total_tree_height: &TreeHeight,
        bottom_hash: HashOutput,
    ) -> Self {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height =
            self.get_height(total_tree_height) - TreeHeight(path_to_bottom.length.0);
        let leftmost_in_subtree = bottom_index.times_two_to_the_power(bottom_height.0);
        let rightmost_in_subtree = leftmost_in_subtree
            + (NodeIndex(Felt::ONE).times_two_to_the_power(bottom_height.0))
            - NodeIndex(Felt::ONE);
        let bottom_leaves =
            &self.sorted_leaf_indices[bisect_left(self.sorted_leaf_indices, &leftmost_in_subtree)
                ..bisect_right(self.sorted_leaf_indices, &rightmost_in_subtree)];

        Self {
            sorted_leaf_indices: bottom_leaves,
            root_index: bottom_index,
            root_hash: bottom_hash,
        }
    }

    fn get_children_subtrees(
        &self,
        left_hash: HashOutput,
        right_hash: HashOutput,
        total_tree_height: &TreeHeight,
    ) -> (Self, Self) {
        let (left_leaves, right_leaves) = self.split_leaves(total_tree_height);
        let left_root_index = self.root_index * Felt::TWO;
        (
            SubTree {
                sorted_leaf_indices: left_leaves,
                root_index: left_root_index,
                root_hash: left_hash,
            },
            SubTree {
                sorted_leaf_indices: right_leaves,
                root_index: left_root_index + NodeIndex(Felt::ONE),
                root_hash: right_hash,
            },
        )
    }

    fn is_leaf(&self, total_tree_height: &TreeHeight) -> bool {
        self.get_height(total_tree_height).0 == 0
    }
}

#[allow(dead_code)]
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
        let subtrees_roots = OriginalSkeletonTreeImpl::calculate_subtrees_roots(
            &subtrees,
            storage,
            &self.tree_height,
        )?;
        for (filled_node, subtree) in subtrees_roots.into_iter().zip(subtrees.iter()) {
            match filled_node.data {
                // Binary node.
                NodeData::Binary(BinaryData {
                    left_hash,
                    right_hash,
                }) => {
                    if subtree.is_sibling() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::LeafOrBinarySibling(filled_node.hash),
                        );
                        continue;
                    }
                    self.nodes
                        .insert(subtree.root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_hash, right_hash, &self.tree_height);
                    next_subtrees.extend(vec![left_subtree, right_subtree]);
                }
                // Edge node.
                NodeData::Edge(EdgeData {
                    bottom_hash,
                    path_to_bottom,
                }) => {
                    if subtree.is_sibling() {
                        // Sibling will remain an edge node. No need to open the bottom.
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::EdgeSibling(EdgeData {
                                bottom_hash,
                                path_to_bottom,
                            }),
                        );
                        continue;
                    }
                    // Parse bottom.
                    let bottom_subtree =
                        subtree.get_bottom_subtree(&path_to_bottom, &self.tree_height, bottom_hash);
                    self.nodes.insert(
                        subtree.root_index,
                        OriginalSkeletonNode::Edge { path_to_bottom },
                    );
                    next_subtrees.push(bottom_subtree);
                }
                // Leaf node.
                NodeData::Leaf(_) => {
                    if subtree.is_sibling() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::LeafOrBinarySibling(filled_node.hash),
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
        total_tree_height: &TreeHeight,
    ) -> OriginalSkeletonTreeResult<Vec<FilledNode<LeafData>>> {
        let mut subtrees_roots = vec![];
        for subtree in subtrees.iter() {
            if subtree.is_leaf(total_tree_height) {
                subtrees_roots.push(FilledNode {
                    hash: subtree.root_hash,
                    // Dummy value that will be ignored.
                    data: NodeData::Leaf(LeafData::StorageValue(Felt::ZERO)),
                });
                continue;
            }
            let key = create_db_key(StoragePrefix::InnerNode, &subtree.root_hash.0.as_bytes());
            let val = storage.get(&key).ok_or(StorageError::MissingKey(key))?;
            subtrees_roots.push(FilledNode::deserialize(
                &StorageKey::from(subtree.root_hash.0),
                val,
            )?)
        }
        Ok(subtrees_roots)
    }
}

impl OriginalSkeletonTree<LeafData> for OriginalSkeletonTreeImpl {
    fn create_tree(
        storage: &impl Storage,
        sorted_leaf_indices: &[NodeIndex],
        root_hash: HashOutput,
        tree_height: TreeHeight,
    ) -> OriginalSkeletonTreeResult<Self> {
        let main_subtree = SubTree {
            sorted_leaf_indices,
            root_index: NodeIndex::root_index(),
            root_hash,
        };
        let mut skeleton_tree = Self {
            nodes: HashMap::new(),
            tree_height,
        };
        skeleton_tree.fetch_nodes(vec![main_subtree], storage)?;
        Ok(skeleton_tree)
    }

    fn compute_updated_skeleton_tree(
        &self,
        _index_to_updated_leaf: HashMap<NodeIndex, LeafData>,
    ) -> OriginalSkeletonTreeResult<UpdatedSkeletonTreeImpl<LeafData>> {
        todo!()
    }
}
