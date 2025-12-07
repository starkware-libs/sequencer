use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::{
    NoCompareOriginalSkeletonTrieConfig,
    OriginalSkeletonTreeConfig,
};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::Storage;
use tracing::warn;

use crate::db::facts_db::traversal::calculate_subtrees_roots;
use crate::db::facts_db::types::FactsSubTree;

#[cfg(test)]
#[path = "create_facts_tree_test.rs"]
pub mod create_facts_tree_test;

/// Logs out a warning of a trivial modification.
macro_rules! log_trivial_modification {
    ($index:expr, $value:expr) => {
        warn!("Encountered a trivial modification at index {:?}, with value {:?}", $index, $value);
    };
}

/// Fetches the Patricia witnesses, required to build the original skeleton tree from storage.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// unmodified nodes. If `compare_modified_leaves` is set, function logs out a warning when
/// encountering a trivial modification. Fills the previous leaf values if it is not none.
async fn fetch_nodes<'a, L: Leaf>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    subtrees: Vec<FactsSubTree<'a>>,
    storage: &mut impl Storage,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig<L>,
    mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<()> {
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    while !current_subtrees.is_empty() {
        let should_fetch_modified_leaves =
            config.compare_modified_leaves() || previous_leaves.is_some();
        let filled_roots =
            calculate_subtrees_roots::<L>(&current_subtrees, storage, key_context).await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.iter()) {
            match filled_root.data {
                // Binary node.
                NodeData::<L, HashOutput>::Binary(BinaryData { left_data, right_data }) => {
                    if subtree.is_unmodified() {
                        skeleton_tree.nodes.insert(
                            subtree.root_index,
                            OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash),
                        );
                        continue;
                    }
                    skeleton_tree.nodes.insert(subtree.root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_data, right_data);

                    handle_subtree(
                        skeleton_tree,
                        &mut next_subtrees,
                        left_subtree,
                        should_fetch_modified_leaves,
                    );
                    handle_subtree(
                        skeleton_tree,
                        &mut next_subtrees,
                        right_subtree,
                        should_fetch_modified_leaves,
                    )
                }
                // Edge node.
                NodeData::<L, HashOutput>::Edge(EdgeData { bottom_data, path_to_bottom }) => {
                    skeleton_tree
                        .nodes
                        .insert(subtree.root_index, OriginalSkeletonNode::Edge(path_to_bottom));
                    if subtree.is_unmodified() {
                        skeleton_tree.nodes.insert(
                            path_to_bottom.bottom_index(subtree.root_index),
                            OriginalSkeletonNode::UnmodifiedSubTree(bottom_data),
                        );
                        continue;
                    }
                    // Parse bottom.
                    let (bottom_subtree, previously_empty_leaves_indices) =
                        subtree.get_bottom_subtree(&path_to_bottom, bottom_data);
                    if let Some(ref mut leaves) = previous_leaves {
                        leaves.extend(
                            previously_empty_leaves_indices
                                .iter()
                                .map(|idx| (**idx, L::default()))
                                .collect::<HashMap<NodeIndex, L>>(),
                        );
                    }
                    log_warning_for_empty_leaves(
                        &previously_empty_leaves_indices,
                        leaf_modifications,
                        config,
                    )?;

                    handle_subtree(
                        skeleton_tree,
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
        current_subtrees = next_subtrees;
        next_subtrees = Vec::new();
    }
    Ok(())
}

pub async fn create_original_skeleton_tree<'a, L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    config: &impl OriginalSkeletonTreeConfig<L>,
    leaf_modifications: &LeafModifications<L>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<OriginalSkeletonTreeImpl<'a>> {
    if sorted_leaf_indices.is_empty() {
        return Ok(OriginalSkeletonTreeImpl::create_unmodified(root_hash));
    }
    if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        log_warning_for_empty_leaves(
            sorted_leaf_indices.get_indices(),
            leaf_modifications,
            config,
        )?;
        return Ok(OriginalSkeletonTreeImpl::create_empty(sorted_leaf_indices));
    }
    let main_subtree = FactsSubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash);
    let mut skeleton_tree = OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };
    fetch_nodes::<L>(
        &mut skeleton_tree,
        vec![main_subtree],
        storage,
        leaf_modifications,
        config,
        None,
        key_context,
    )
    .await?;
    Ok(skeleton_tree)
}

pub async fn create_original_skeleton_tree_and_get_previous_leaves<'a, L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig<L>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, L>)> {
    if sorted_leaf_indices.is_empty() {
        let unmodified = OriginalSkeletonTreeImpl::create_unmodified(root_hash);
        return Ok((unmodified, HashMap::new()));
    }
    if root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok((
            OriginalSkeletonTreeImpl::create_empty(sorted_leaf_indices),
            sorted_leaf_indices.get_indices().iter().map(|idx| (*idx, L::default())).collect(),
        ));
    }
    let main_subtree = FactsSubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash);
    let mut skeleton_tree = OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };
    let mut leaves = HashMap::new();
    fetch_nodes::<L>(
        &mut skeleton_tree,
        vec![main_subtree],
        storage,
        leaf_modifications,
        config,
        Some(&mut leaves),
        key_context,
    )
    .await?;
    Ok((skeleton_tree, leaves))
}

pub async fn get_leaves<'a, L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<HashMap<NodeIndex, L>> {
    let config = NoCompareOriginalSkeletonTrieConfig::default();
    let leaf_modifications = LeafModifications::new();
    let (_, previous_leaves) = create_original_skeleton_tree_and_get_previous_leaves(
        storage,
        root_hash,
        sorted_leaf_indices,
        &leaf_modifications,
        &config,
        key_context,
    )
    .await?;
    Ok(previous_leaves)
}

/// Handles a subtree referred by an edge or a binary node. Decides whether we deserialize the
/// referred subtree or not.
fn handle_subtree<'a>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    next_subtrees: &mut Vec<FactsSubTree<'a>>,
    subtree: FactsSubTree<'a>,
    should_fetch_modified_leaves: bool,
) {
    if !subtree.is_leaf() || (should_fetch_modified_leaves && !subtree.is_unmodified()) {
        next_subtrees.push(subtree);
    } else if subtree.is_unmodified() {
        // Leaf sibling.
        skeleton_tree
            .nodes
            .insert(subtree.root_index, OriginalSkeletonNode::UnmodifiedSubTree(subtree.root_hash));
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
