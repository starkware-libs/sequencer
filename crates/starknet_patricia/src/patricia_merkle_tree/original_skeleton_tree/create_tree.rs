use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;

use starknet_patricia_storage::storage_trait::Storage;
use tracing::warn;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData, NodeData};
use crate::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use crate::patricia_merkle_tree::traversal::{calculate_subtrees_roots, SubTree};
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};

#[cfg(test)]
#[path = "create_tree_test.rs"]
pub mod create_tree_test;

/// Logs out a warning of a trivial modification.
macro_rules! log_trivial_modification {
    ($index:expr, $value:expr) => {
        warn!("Encountered a trivial modification at index {:?}, with value {:?}", $index, $value);
    };
}

impl<'a> OriginalSkeletonTreeImpl<'a> {
    /// Fetches the Patricia witnesses, required to build the original skeleton tree from storage.
    /// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
    /// unmodified nodes. If `compare_modified_leaves` is set, function logs out a warning when
    /// encountering a trivial modification. Fills the previous leaf values if it is not none.
    fn fetch_nodes<L: Leaf>(
        &mut self,
        subtrees: Vec<SubTree<'a>>,
        storage: &impl Storage,
        leaf_modifications: &LeafModifications<L>,
        config: &impl OriginalSkeletonTreeConfig<L>,
        mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    ) -> OriginalSkeletonTreeResult<()> {
        if subtrees.is_empty() {
            return Ok(());
        }
        let should_fetch_modified_leaves =
            config.compare_modified_leaves() || previous_leaves.is_some();
        let mut next_subtrees = Vec::new();
        let filled_roots = calculate_subtrees_roots::<L>(&subtrees, storage)?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(subtrees.iter()) {
            match filled_root.data {
                // Binary node.
                NodeData::Binary(BinaryData { left_hash, right_hash }) => {
                    if subtree.is_unmodified() {
                        self.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                        continue;
                    }
                    self.nodes.insert(subtree.root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_hash, right_hash);

                    self.handle_subtree(
                        &mut next_subtrees,
                        left_subtree,
                        should_fetch_modified_leaves,
                    );
                    self.handle_subtree(
                        &mut next_subtrees,
                        right_subtree,
                        should_fetch_modified_leaves,
                    )
                }
                // Edge node.
                NodeData::Edge(EdgeData { bottom_hash, path_to_bottom }) => {
                    self.nodes
                        .insert(subtree.root_index, OriginalSkeletonNode::Edge(path_to_bottom));
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
                    OriginalSkeletonTreeImpl::log_warning_for_empty_leaves(
                        &previously_empty_leaves_indices,
                        leaf_modifications,
                        config,
                    )?;

                    self.handle_subtree(
                        &mut next_subtrees,
                        bottom_subtree,
                        should_fetch_modified_leaves,
                    );
                }
                // Leaf node.
                NodeData::Leaf(previous_leaf) => {
                    if subtree.is_unmodified() {
                        warn!("Unexpectedly deserialized leaf sibling.")
                    } else {
                        // Modified leaf.
                        if config.compare_modified_leaves()
                            && L::compare(leaf_modifications, &subtree.root_index, &previous_leaf)?
                        {
                            log_trivial_modification!(subtree.root_index, previous_leaf);
                        }
                        // If previous values of modified leaves are requested, add this leaf.
                        if let Some(ref mut leaves) = previous_leaves {
                            leaves.insert(subtree.root_index, previous_leaf);
                        }
                    }
                }
            }
        }
        self.fetch_nodes::<L>(next_subtrees, storage, leaf_modifications, config, previous_leaves)
    }

    pub(crate) fn create_impl<L: Leaf>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &impl OriginalSkeletonTreeConfig<L>,
        leaf_modifications: &LeafModifications<L>,
    ) -> OriginalSkeletonTreeResult<Self> {
        if sorted_leaf_indices.is_empty() {
            return Ok(Self::create_unmodified(root_hash));
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            OriginalSkeletonTreeImpl::log_warning_for_empty_leaves(
                sorted_leaf_indices.get_indices(),
                leaf_modifications,
                config,
            )?;
            return Ok(Self::create_empty(sorted_leaf_indices));
        }
        let main_subtree = SubTree { sorted_leaf_indices, root_index: NodeIndex::ROOT, root_hash };
        let mut skeleton_tree = Self { nodes: HashMap::new(), sorted_leaf_indices };
        skeleton_tree.fetch_nodes::<L>(
            vec![main_subtree],
            storage,
            leaf_modifications,
            config,
            None,
        )?;
        Ok(skeleton_tree)
    }

    pub(crate) fn create_and_get_previous_leaves_impl<L: Leaf>(
        storage: &impl Storage,
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        leaf_modifications: &LeafModifications<L>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)> {
        if sorted_leaf_indices.is_empty() {
            let unmodified = Self::create_unmodified(root_hash);
            return Ok((unmodified, HashMap::new()));
        }
        if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
            return Ok((
                Self::create_empty(sorted_leaf_indices),
                sorted_leaf_indices.get_indices().iter().map(|idx| (*idx, L::default())).collect(),
            ));
        }
        let main_subtree = SubTree { sorted_leaf_indices, root_index: NodeIndex::ROOT, root_hash };
        let mut skeleton_tree = Self { nodes: HashMap::new(), sorted_leaf_indices };
        let mut leaves = HashMap::new();
        skeleton_tree.fetch_nodes::<L>(
            vec![main_subtree],
            storage,
            leaf_modifications,
            config,
            Some(&mut leaves),
        )?;
        Ok((skeleton_tree, leaves))
    }

    fn create_unmodified(root_hash: HashOutput) -> Self {
        Self {
            nodes: HashMap::from([(
                NodeIndex::ROOT,
                OriginalSkeletonNode::UnmodifiedSubTree(root_hash),
            )]),
            sorted_leaf_indices: SortedLeafIndices::default(),
        }
    }

    pub(crate) fn create_empty(sorted_leaf_indices: SortedLeafIndices<'a>) -> Self {
        Self { nodes: HashMap::new(), sorted_leaf_indices }
    }

    /// Handles a subtree referred by an edge or a binary node. Decides whether we deserialize the
    /// referred subtree or not.
    fn handle_subtree(
        &mut self,
        next_subtrees: &mut Vec<SubTree<'a>>,
        subtree: SubTree<'a>,
        should_fetch_modified_leaves: bool,
    ) {
        if !subtree.is_leaf() || (should_fetch_modified_leaves && !subtree.is_unmodified()) {
            next_subtrees.push(subtree);
        } else if subtree.is_unmodified() {
            // Leaf sibling.
            self.nodes.insert(
                subtree.root_index,
                OriginalSkeletonNode::UnmodifiedSubTree(subtree.root_hash),
            );
        }
    }

    /// Given leaf indices that were previously empty leaves, logs out a warning for trivial
    /// modification if a leaf is modified to an empty leaf.
    /// If this check is suppressed by configuration, does nothing.
    fn log_warning_for_empty_leaves<L: Leaf, T: Borrow<NodeIndex> + Debug>(
        leaf_indices: &[T],
        leaf_modifications: &LeafModifications<L>,
        config: &impl OriginalSkeletonTreeConfig<L>,
    ) -> OriginalSkeletonTreeResult<()> {
        if !config.compare_modified_leaves() {
            return Ok(());
        }
        let empty_leaf = L::default();
        for leaf_index in leaf_indices {
            if L::compare(leaf_modifications, leaf_index.borrow(), &empty_leaf)? {
                log_trivial_modification!(leaf_index, empty_leaf);
            }
        }
        Ok(())
    }
}
