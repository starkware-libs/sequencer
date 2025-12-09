use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::traversal::{SubTreeTrait, UnmodifiedChildTraversal};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{DBObject, HasStaticPrefix};
use starknet_patricia_storage::storage_trait::Storage;
use tracing::warn;

use crate::db::db_layout::NodeLayout;
use crate::db::facts_db::db::FactsNodeLayout;
use crate::db::facts_db::traversal::get_roots_from_storage;
use crate::db::facts_db::types::FactsSubTree;
use crate::patricia_merkle_tree::tree::OriginalSkeletonTrieDontCompareConfig;

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
///
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// unmodified nodes. If `compare_modified_leaves` is set, function logs out a warning when
/// encountering a trivial modification. Fills the previous leaf values if it is not none.
///
/// The function is generic over the DB layout (`Layout`), which controls the concrete node data
/// (`Layout::NodeData`) and traversal strategy (via `Layout::SubTree`).
async fn fetch_nodes<'a, L, Layout>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    subtrees: Vec<Layout::SubTree>,
    storage: &mut impl Storage,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig,
    mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<()>
where
    L: Leaf,
    Layout: NodeLayout<'a, L>,
    FilledNode<L, Layout::NodeData>: DBObject<DeserializeContext = Layout::DeserializationContext>,
{
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    let should_fetch_modified_leaves =
        config.compare_modified_leaves() || previous_leaves.is_some();
    while !current_subtrees.is_empty() {
        let filled_roots =
            get_roots_from_storage::<L, Layout>(&current_subtrees, storage, key_context).await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.into_iter()) {
            if subtree.is_unmodified() {
                handle_unmodified_subtree(skeleton_tree, &mut next_subtrees, filled_root, subtree);
                continue;
            }
            let root_index = subtree.get_root_index();
            match filled_root.data {
                // Binary node.
                NodeData::Binary(BinaryData { left_data, right_data }) => {
                    skeleton_tree.nodes.insert(root_index, OriginalSkeletonNode::Binary);
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_data, right_data);

                    handle_child_subtree(
                        skeleton_tree,
                        &mut next_subtrees,
                        left_subtree,
                        left_data,
                        should_fetch_modified_leaves,
                    );
                    handle_child_subtree(
                        skeleton_tree,
                        &mut next_subtrees,
                        right_subtree,
                        right_data,
                        should_fetch_modified_leaves,
                    )
                }
                // Edge node.
                NodeData::Edge(EdgeData { bottom_data, path_to_bottom }) => {
                    skeleton_tree
                        .nodes
                        .insert(root_index, OriginalSkeletonNode::Edge(path_to_bottom));

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

                    handle_child_subtree(
                        skeleton_tree,
                        &mut next_subtrees,
                        bottom_subtree,
                        bottom_data,
                        should_fetch_modified_leaves,
                    );
                }
                // Leaf node.
                NodeData::Leaf(previous_leaf) => {
                    // Modified leaf.
                    if config.compare_modified_leaves()
                        && L::compare(leaf_modifications, &root_index, &previous_leaf)?
                    {
                        log_trivial_modification!(root_index, previous_leaf);
                    }
                    // If previous values of modified leaves are requested, add this leaf.
                    if let Some(ref mut leaves) = previous_leaves {
                        leaves.insert(root_index, previous_leaf);
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
    config: &impl OriginalSkeletonTreeConfig,
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
    fetch_nodes::<L, FactsNodeLayout>(
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
    config: &impl OriginalSkeletonTreeConfig,
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
    fetch_nodes::<L, FactsNodeLayout>(
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

/// Prepares the OS inputs by fetching paths to the given leaves (i.e. their induced Skeleton tree).
/// Note that ATM, the Rust committer does not manage history and is not used for storage proofs;
/// Thus, this function assumes facts layout.
pub async fn get_leaves<'a, L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<HashMap<NodeIndex, L>> {
    let config = OriginalSkeletonTrieDontCompareConfig;
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

/// Adds the subtree root to the skeleton tree. If the root is an edge node, and
/// `should_traverse_unmodified_child` is `Skip`, then the corresponding bottom node is also
/// added to the skeleton. Otherwise, the bottom subtree is added to the next subtrees.
fn handle_unmodified_subtree<'a, L: Leaf, SubTree: SubTreeTrait<'a>>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    next_subtrees: &mut Vec<SubTree>,
    filled_root: FilledNode<L, SubTree::NodeData>,
    subtree: SubTree,
) where
    SubTree::NodeData: Copy,
{
    // Sanity check.
    assert!(subtree.is_unmodified(), "Called `handle_unmodified_subtree` for a modified subtree.");

    let root_index = subtree.get_root_index();

    if let NodeData::Edge(EdgeData { bottom_data, path_to_bottom }) = filled_root.data {
        // Even if a subtree rooted at an edge node is unmodified, we still need an
        // `OriginalSkeletonNode::Edge` node in the skeleton in case we need to manipulate it later
        // (e.g. unify the edge node with an ancestor edge node).
        skeleton_tree.nodes.insert(root_index, OriginalSkeletonNode::Edge(path_to_bottom));
        match SubTree::should_traverse_unmodified_child(bottom_data) {
            UnmodifiedChildTraversal::Traverse => {
                let (bottom_subtree, _) = subtree.get_bottom_subtree(&path_to_bottom, bottom_data);
                next_subtrees.push(bottom_subtree);
            }
            UnmodifiedChildTraversal::Skip(unmodified_child_hash) => {
                skeleton_tree.nodes.insert(
                    path_to_bottom.bottom_index(root_index),
                    OriginalSkeletonNode::UnmodifiedSubTree(unmodified_child_hash),
                );
            }
        }
    } else {
        skeleton_tree
            .nodes
            .insert(root_index, OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash));
    }
}

/// Handles a subtree referred by an edge or a binary node. Decides whether we deserialize the
/// referred subtree or not, and if we should continue traversing the child's direction.
fn handle_child_subtree<'a, SubTree: SubTreeTrait<'a>>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    next_subtrees: &mut Vec<SubTree>,
    subtree: SubTree,
    subtree_data: SubTree::NodeData,
    should_fetch_modified_leaves: bool,
) {
    let is_leaf = subtree.is_leaf();
    let is_modified = !subtree.is_unmodified();

    // Internal node → always traverse.
    if !is_leaf {
        next_subtrees.push(subtree);
        return;
    }

    // Modified leaf.
    if is_modified {
        if should_fetch_modified_leaves {
            next_subtrees.push(subtree);
        }
        return;
    }

    // Unmodified leaf sibling.
    match SubTree::should_traverse_unmodified_child(subtree_data) {
        UnmodifiedChildTraversal::Traverse => {
            next_subtrees.push(subtree);
        }
        UnmodifiedChildTraversal::Skip(unmodified_child_hash) => {
            skeleton_tree.nodes.insert(
                subtree.get_root_index(),
                OriginalSkeletonNode::UnmodifiedSubTree(unmodified_child_hash),
            );
        }
    }
}

/// Given leaf indices that were previously empty leaves, logs out a warning for trivial
/// modification if a leaf is modified to an empty leaf.
/// If this check is suppressed by configuration, does nothing.
fn log_warning_for_empty_leaves<L: Leaf, T: Borrow<NodeIndex> + Debug>(
    leaf_indices: &[T],
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig,
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
