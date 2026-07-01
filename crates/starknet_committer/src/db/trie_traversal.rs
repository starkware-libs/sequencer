use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
    PathToBottom,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::traversal::{
    SubTreeTrait,
    TraversalResult,
    UnmodifiedChildTraversal,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::reads_collector_storage::ReadsCollectorStorage;
use starknet_patricia_storage::storage_trait::{
    DbKey,
    GatherableStorage,
    ImmutableReadOnlyStorage,
    ReadOnlyStorage,
    Storage,
    StorageTask,
    StorageTaskOutput,
};
use tracing::warn;

use crate::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
    ReaderConfig,
    StarknetStorageValue,
};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::OriginalSkeletonTrieConfig;
use crate::patricia_merkle_tree::types::CompiledClassHash;

#[cfg(test)]
#[path = "fetch_patricia_paths_tests.rs"]
mod fetch_patricia_paths_tests;

/// Returns the Patricia inner nodes ([PreimageMap]) in the paths to the given `leaf_indices` in the
/// given tree according to the `root_hash` (including siblings).
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
pub async fn fetch_patricia_paths<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    storage: &mut impl ReadOnlyStorage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<PreimageMap> {
    let mut witnesses = PreimageMap::new();

    if sorted_leaf_indices.is_empty() || root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok(witnesses);
    }

    let main_subtree =
        Layout::SubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash.into());

    fetch_patricia_paths_inner::<L, Layout>(
        storage,
        vec![main_subtree],
        &mut witnesses,
        leaves,
        key_context,
    )
    .await?;
    Ok(witnesses)
}

/// Fetches the inner nodes [HashOutput] and [Preimage] in the paths to modified leaves.
/// Required for `patricia_update` function in Cairo.
/// Extra preimages (more than the data required to verify merkle paths) are required to verify
/// correctness of final tree topology; for more details see 'traverse_edge' in 'patricia.cairo'.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// inner nodes in their paths and their siblings.
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
pub(crate) async fn fetch_patricia_paths_inner<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    storage: &mut impl ReadOnlyStorage,
    subtrees: Vec<Layout::SubTree>,
    witnesses: &mut PreimageMap,
    mut leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<()> {
    // Hashes collected so far, keyed by node index.
    let mut hash_by_index: HashMap<NodeIndex, HashOutput> = HashMap::new();
    // Pending binary preimage entries: (node_hash, left_index, right_index).
    let mut pending_binary: Vec<(HashOutput, NodeIndex, NodeIndex)> = Vec::new();
    // Pending edge preimage entries: (node_hash, path, bottom_index).
    let mut pending_edge: Vec<(HashOutput, PathToBottom, NodeIndex)> = Vec::new();

    // `current_traversal`: subtrees requiring full processing (preimage building + traversal).
    // `next_hash_only`: unmodified subtrees queued to load their hash (no traversal) before the
    // next round's pendings flush.
    let mut current_traversal = subtrees;
    let mut next_traversal: Vec<Layout::SubTree> = Vec::new();
    let mut next_hash_only: Vec<Layout::SubTree> = Vec::new();

    while !current_traversal.is_empty() {
        let filled_roots =
            get_roots_from_storage::<L, Layout>(&current_traversal, storage, key_context).await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_traversal.iter()) {
            hash_by_index.insert(subtree.get_root_index(), filled_root.hash);
            match filled_root.data {
                // Binary node.
                // If it's the root: It's a modified subtree.
                // Otherwise:
                // If it was inserted as a child of a binary node - it's a modified subtree.
                // If it was inserted as a child of an edge node - it should be fetched anyway
                // (modified or unmodified).
                NodeData::Binary(BinaryData { left_data, right_data }) => {
                    let (left_subtree, right_subtree) =
                        subtree.get_children_subtrees(left_data.clone(), right_data.clone());
                    let left_index = left_subtree.get_root_index();
                    let right_index = right_subtree.get_root_index();

                    let left_hash = schedule_binary_child::<L, Layout>(
                        left_data,
                        left_subtree,
                        &mut hash_by_index,
                        &mut next_traversal,
                        &mut next_hash_only,
                    );
                    let right_hash = schedule_binary_child::<L, Layout>(
                        right_data,
                        right_subtree,
                        &mut hash_by_index,
                        &mut next_traversal,
                        &mut next_hash_only,
                    );

                    if let (Some(left_hash), Some(right_hash)) = (left_hash, right_hash) {
                        witnesses.insert(
                            filled_root.hash,
                            Preimage::Binary(BinaryData {
                                left_data: left_hash,
                                right_data: right_hash,
                            }),
                        );
                    } else {
                        pending_binary.push((filled_root.hash, left_index, right_index));
                    }
                }
                // Edge node.
                // If it's the root: it's not necessarily a modified tree, because the modification
                // might be a deletion. In this case, we want to fetch the bottom node only if it's
                // a binary node (and not a leaf). Otherwise: It was inserted as a child of a binary
                // node, so it's a modified subtree.
                NodeData::Edge(EdgeData { bottom_data, path_to_bottom }) => {
                    let (bottom_subtree, empty_leaves_indices) =
                        subtree.get_bottom_subtree(&path_to_bottom, bottom_data.clone());
                    let bottom_index = bottom_subtree.get_root_index();

                    if let Some(ref mut leaves_map) = leaves {
                        // Insert empty leaves descendent of the current subtree, that are not
                        // descendents of the bottom node.
                        for index in empty_leaves_indices {
                            leaves_map.insert(*index, L::default());
                        }
                    }

                    let bottom_hash = schedule_edge_child::<L, Layout>(
                        bottom_data,
                        bottom_subtree,
                        &mut hash_by_index,
                        &mut next_traversal,
                    );

                    match bottom_hash {
                        Some(hash) => {
                            witnesses.insert(
                                filled_root.hash,
                                Preimage::Edge(EdgeData { bottom_data: hash, path_to_bottom }),
                            );
                        }
                        None => {
                            pending_edge.push((filled_root.hash, path_to_bottom, bottom_index));
                        }
                    }
                }
                // Leaf node.
                // If it was inserted as a child of a binary node - it's a modified leaf.
                // If it was inserted as a child of an edge node - it means the edge node is
                // modified, meaning the leaf is also modified.
                NodeData::Leaf(leaf_data) => {
                    if let Some(ref mut leaves_map) = leaves {
                        if !subtree.is_unmodified() {
                            leaves_map.insert(subtree.get_root_index(), leaf_data);
                        }
                    }
                }
            }
        }

        clear_pending_nodes::<L, Layout>(
            &mut pending_binary,
            &mut pending_edge,
            &next_hash_only,
            storage,
            key_context,
            &mut hash_by_index,
            witnesses,
        )
        .await?;

        current_traversal = next_traversal;
        next_traversal = Vec::new();
        next_hash_only = Vec::new();
    }

    Ok(())
}

/// Loads hashes for `hash_only_subtrees` into the given `hash_by_index`.
async fn read_hashes<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    hash_only_subtrees: &[Layout::SubTree],
    storage: &mut impl ReadOnlyStorage,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
) -> TraversalResult<()> {
    let filled_roots =
        get_roots_from_storage::<L, Layout>(hash_only_subtrees, storage, key_context).await?;
    for (filled_root, subtree) in filled_roots.into_iter().zip(hash_only_subtrees.iter()) {
        hash_by_index.insert(subtree.get_root_index(), filled_root.hash);
    }
    Ok(())
}

/// Flushes [`PreimageMap`] entries that were waiting on child hashes from storage.
async fn clear_pending_nodes<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    pending_binary: &mut Vec<(HashOutput, NodeIndex, NodeIndex)>,
    pending_edge: &mut Vec<(HashOutput, PathToBottom, NodeIndex)>,
    hash_only_subtrees: &[Layout::SubTree],
    storage: &mut impl ReadOnlyStorage,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
    witnesses: &mut PreimageMap,
) -> TraversalResult<()> {
    read_hashes::<L, Layout>(hash_only_subtrees, storage, key_context, hash_by_index).await?;

    pending_binary.retain(|&(node_hash, left_index, right_index)| {
        match (hash_by_index.get(&left_index), hash_by_index.get(&right_index)) {
            (Some(&left_hash), Some(&right_hash)) => {
                witnesses.insert(
                    node_hash,
                    Preimage::Binary(BinaryData { left_data: left_hash, right_data: right_hash }),
                );
                false
            }
            // Children hashes are not yet available, so we keep the entry in the pending queue.
            // Not possible in facts layout. In index layout, this occurs when at least one of the
            // children has modified leaves, hence its hash will only be available in
            // the next iteration.
            _ => true,
        }
    });
    pending_edge.retain(|&(node_hash, path_to_bottom, bottom_index)| {
        if let Some(&bottom_hash) = hash_by_index.get(&bottom_index) {
            witnesses.insert(
                node_hash,
                Preimage::Edge(EdgeData { bottom_data: bottom_hash, path_to_bottom }),
            );
            false
        }
        // Bottom hash is not yet available, so we keep the entry in the pending queue.
        // Not possible in facts layout. In index layout, edge nodes remain pending until the next
        // iteration, when the bottom node is traversed.
        else {
            true
        }
    });

    Ok(())
}

/// Handles a left/right child under a binary node in [`fetch_patricia_paths_inner`].
///
/// On [`UnmodifiedChildTraversal::Skip`], pushes the child onto `next_traversal` when the subtree
/// is modified.
///
/// On [`UnmodifiedChildTraversal::Traverse`], unmodified subtrees added to the `next_hash_only`
/// queue; modified subtrees use enqueued for further traversal.
///
/// Returns the child hash if immediately available, or `None` if the child must be read first.
fn schedule_binary_child<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    node_data: Layout::NodeData,
    subtree: Layout::SubTree,
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
    next_traversal: &mut Vec<Layout::SubTree>,
    next_hash_only: &mut Vec<Layout::SubTree>,
) -> Option<HashOutput> {
    match Layout::SubTree::should_traverse_unmodified_child(node_data) {
        UnmodifiedChildTraversal::Skip(hash) => {
            hash_by_index.insert(subtree.get_root_index(), hash);
            if !subtree.is_unmodified() {
                next_traversal.push(subtree);
            }
            Some(hash)
        }
        UnmodifiedChildTraversal::Traverse => {
            if subtree.is_unmodified() {
                next_hash_only.push(subtree);
            } else {
                next_traversal.push(subtree);
            }
            None
        }
    }
}

/// Handles the bottom child under an edge node in [`fetch_patricia_paths_inner`].
///
/// On [`UnmodifiedChildTraversal::Skip`], pushes onto `next_traversal` (unless it's an unmodified
/// leaf), as we always need the bottom preimage.
///
/// On [`UnmodifiedChildTraversal::Traverse`], enqueues for further traversal.
///
/// Returns the child hash if immediately available, or `None` if the child must be read first.
fn schedule_edge_child<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    node_data: Layout::NodeData,
    subtree: Layout::SubTree,
    hash_by_index: &mut HashMap<NodeIndex, HashOutput>,
    next_traversal: &mut Vec<Layout::SubTree>,
) -> Option<HashOutput> {
    match Layout::SubTree::should_traverse_unmodified_child(node_data) {
        UnmodifiedChildTraversal::Skip(hash) => {
            hash_by_index.insert(subtree.get_root_index(), hash);
            if !subtree.is_unmodified() || !subtree.is_leaf() {
                next_traversal.push(subtree);
            }
            Some(hash)
        }
        UnmodifiedChildTraversal::Traverse => {
            next_traversal.push(subtree);
            None
        }
    }
}

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
pub(crate) async fn fetch_nodes<'a, L, Layout>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    subtrees: Vec<Layout::SubTree>,
    storage: &mut impl ReadOnlyStorage,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig,
    mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<()>
where
    L: Leaf,
    Layout: NodeLayout<'a, L>,
{
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    let should_fetch_modified_leaves =
        config.compare_modified_leaves() || previous_leaves.is_some();
    while !current_subtrees.is_empty() {
        let filled_roots =
            get_roots_from_storage::<L, Layout>(&current_subtrees, storage, key_context).await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees) {
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
                        subtree.get_children_subtrees(left_data.clone(), right_data.clone());

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
                        subtree.get_bottom_subtree(&path_to_bottom, bottom_data.clone());

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

/// Adds the subtree root to the skeleton tree. If the root is an edge node, and
/// `should_traverse_unmodified_child` is `Skip`, then the corresponding bottom node is also
/// added to the skeleton. Otherwise, the bottom subtree is added to the next subtrees.
fn handle_unmodified_subtree<'a, L: Leaf, SubTree: SubTreeTrait<'a>>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    next_subtrees: &mut Vec<SubTree>,
    filled_root: FilledNode<L, SubTree::NodeData>,
    subtree: SubTree,
) where
    SubTree::NodeData: Clone,
{
    // Sanity check.
    assert!(subtree.is_unmodified(), "Called `handle_unmodified_subtree` for a modified subtree.");

    let root_index = subtree.get_root_index();

    match filled_root.data {
        NodeData::Edge(EdgeData { bottom_data, path_to_bottom }) => {
            // Even if a subtree rooted at an edge node is unmodified, we still need an
            // `OriginalSkeletonNode::Edge` node in the skeleton in case we need to manipulate it
            // later (e.g. unify the edge node with an ancestor edge node).
            skeleton_tree.nodes.insert(root_index, OriginalSkeletonNode::Edge(path_to_bottom));
            match SubTree::should_traverse_unmodified_child(bottom_data.clone()) {
                UnmodifiedChildTraversal::Traverse => {
                    let (bottom_subtree, _) =
                        subtree.get_bottom_subtree(&path_to_bottom, bottom_data);
                    next_subtrees.push(bottom_subtree);
                }
                UnmodifiedChildTraversal::Skip(unmodified_child_hash) => {
                    skeleton_tree.nodes.insert(
                        path_to_bottom.bottom_index(root_index),
                        OriginalSkeletonNode::UnmodifiedSubTree(unmodified_child_hash),
                    );
                }
            }
        }
        NodeData::Binary(_) | NodeData::Leaf(_) => {
            skeleton_tree
                .nodes
                .insert(root_index, OriginalSkeletonNode::UnmodifiedSubTree(filled_root.hash));
        }
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
    let is_modified = !subtree.is_unmodified();

    // Internal node → always traverse.
    if !subtree.is_leaf() {
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

// TODO(Aviv, 17/07/2024): Split between storage prefix implementation and function logic.
pub async fn get_roots_from_storage<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    subtrees: &[Layout::SubTree],
    storage: &mut impl ReadOnlyStorage,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<Vec<FilledNode<L, Layout::NodeData>>> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> =
        subtrees.iter().map(|subtree| subtree.get_root_db_key::<L>(key_context)).collect();

    let db_vals = storage.mget_mut(&db_keys.iter().collect::<Vec<&DbKey>>()).await?;
    for ((subtree, optional_val), db_key) in subtrees.iter().zip(db_vals.iter()).zip(db_keys) {
        let Some(val) = optional_val else { Err(StorageError::MissingKey(db_key))? };
        let filled_node =
            Layout::NodeDbObject::deserialize(val, &subtree.get_root_context())?.into();
        subtrees_roots.push(filled_node);
    }
    Ok(subtrees_roots)
}

/// Given leaf indices that were previously empty leaves, logs out a warning for trivial
/// modification if a leaf is modified to an empty leaf.
/// If this check is suppressed by configuration, does nothing.
pub(crate) fn log_warning_for_empty_leaves<L: Leaf, T: Borrow<NodeIndex> + Debug>(
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

/// Creates an original skeleton tree by fetching Patricia nodes from storage.
///
/// Traverses the trie from the root towards the modified leaves, collecting all nodes needed
/// to construct the skeleton. If `previous_leaves` is provided, it will be populated with the
/// previous values of modified leaves.
///
/// # Layout-Dependent Arguments
///
/// The following arguments depend on the `Layout` type parameter:
/// - `storage`: storage to fetch nodes from, expected to match the serialization of
///   `Layout::NodeDbObject`.
/// - `key_context`: additional context that is needed to determine the DB key prefix used when
///   fetching nodes from storage.
/// - `leaf_modifications` and `previous_leaves`: Their leaf type `L` is constrained by `Layout`.
pub async fn create_original_skeleton_tree<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    storage: &mut impl ReadOnlyStorage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    config: &impl OriginalSkeletonTreeConfig,
    leaf_modifications: &LeafModifications<L>,
    previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
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
        if let Some(previous_leaves) = previous_leaves {
            previous_leaves.extend(
                sorted_leaf_indices
                    .get_indices()
                    .iter()
                    .map(|idx| (*idx, L::default()))
                    .collect::<HashMap<NodeIndex, L>>(),
            );
        }
        return Ok(OriginalSkeletonTreeImpl::create_empty(sorted_leaf_indices));
    }
    let main_subtree =
        Layout::SubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash.into());
    let mut skeleton_tree = OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };
    fetch_nodes::<L, Layout>(
        &mut skeleton_tree,
        vec![main_subtree],
        storage,
        leaf_modifications,
        config,
        previous_leaves,
        key_context,
    )
    .await?;
    Ok(skeleton_tree)
}

/// Creates the original skeleton trees of the modified storage tries.
/// If [ReaderConfig::build_storage_tries_concurrently] is enabled, and the storage layout is
/// [GatherableStorage], the tries are created concurrently. Otherwise, they are created
/// sequentially.
pub async fn create_storage_tries<'a, Layout>(
    storage: &mut impl Storage,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    config: &ReaderConfig,
    storage_tries_sorted_indices: &'a HashMap<ContractAddress, SortedLeafIndices<'a>>,
) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>>
where
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    if config.build_storage_tries_concurrently() {
        if let Some(gatherable_storage) = storage.as_gatherable_storage() {
            return create_storage_tries_concurrently::<_, Layout>(
                gatherable_storage,
                actual_storage_updates,
                original_contracts_trie_leaves,
                config.warn_on_trivial_modifications(),
                storage_tries_sorted_indices,
            )
            .await;
        } else {
            warn!(
                "Concurrent storage tries creation is enabled in config but the storage layer \
                 doesn't support it. Creating storage tries sequentially..."
            );
        }
    }
    create_storage_tries_sequentially::<Layout>(
        storage,
        actual_storage_updates,
        original_contracts_trie_leaves,
        config,
        storage_tries_sorted_indices,
    )
    .await
}

/// Creates the contracts trie original skeleton.
/// Also returns the previous contracts state of the modified contracts.
pub async fn create_contracts_trie<'a, Layout: NodeLayoutFor<ContractState>>(
    storage: &mut impl ReadOnlyStorage,
    contracts_trie_root_hash: HashOutput,
    contracts_trie_sorted_indices: SortedLeafIndices<'a>,
) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)>
where
    <Layout as NodeLayoutFor<ContractState>>::DbLeaf: HasStaticPrefix<KeyContext = EmptyKeyContext>,
{
    let config = OriginalSkeletonTrieConfig::new_for_contracts_trie();

    let mut leaves = HashMap::new();
    let skeleton_tree = create_original_skeleton_tree::<Layout::DbLeaf, Layout>(
        storage,
        contracts_trie_root_hash,
        contracts_trie_sorted_indices,
        &config,
        &HashMap::new(),
        Some(&mut leaves),
        &EmptyKeyContext,
    )
    .await?;

    let leaves: HashMap<NodeIndex, ContractState> =
        leaves.into_iter().map(|(idx, leaf)| (idx, leaf.into())).collect();

    Ok((skeleton_tree, leaves))
}

pub async fn create_classes_trie<'a, Layout: NodeLayoutFor<CompiledClassHash>>(
    storage: &mut impl ReadOnlyStorage,
    actual_classes_updates: &LeafModifications<CompiledClassHash>,
    classes_trie_root_hash: HashOutput,
    config: &ReaderConfig,
    contracts_trie_sorted_indices: SortedLeafIndices<'a>,
) -> ForestResult<OriginalSkeletonTreeImpl<'a>>
where
    <Layout as NodeLayoutFor<CompiledClassHash>>::DbLeaf:
        HasStaticPrefix<KeyContext = EmptyKeyContext>,
{
    let config = OriginalSkeletonTrieConfig::new_for_classes_or_storage_trie(
        config.warn_on_trivial_modifications(),
    );

    Ok(create_original_skeleton_tree::<Layout::DbLeaf, Layout>(
        storage,
        classes_trie_root_hash,
        contracts_trie_sorted_indices,
        &config,
        // TODO(Ariel): Change `actual_classes_updates` to be an iterator over borrowed data so
        // that the conversion below is costless.
        &actual_classes_updates
            .iter()
            .map(|(idx, value)| (*idx, Layout::DbLeaf::from(*value)))
            .collect(),
        None,
        &EmptyKeyContext,
    )
    .await?)
}

async fn create_storage_tries_sequentially<'a, Layout: NodeLayoutFor<StarknetStorageValue>>(
    storage: &mut impl ReadOnlyStorage,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    config: &ReaderConfig,
    storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>>
where
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    let mut storage_tries = HashMap::new();
    for (address, updates) in actual_storage_updates {
        let sorted_leaf_indices = storage_tries_sorted_indices
            .get(address)
            .ok_or(ForestError::MissingSortedLeafIndices(*address))?;
        let storage_root_hash = original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .ok_or(ForestError::MissingContractCurrentState(*address))?
            .storage_root_hash;
        let original_skeleton = create_storage_trie::<Layout>(
            storage,
            *address,
            updates,
            storage_root_hash,
            *sorted_leaf_indices,
            config.warn_on_trivial_modifications(),
        )
        .await?;
        storage_tries.insert(*address, original_skeleton);
    }
    Ok(storage_tries)
}

/// Holds all data needed to build one storage trie concurrently.
///
/// Two distinct lifetimes are needed:
/// - `'indices` is the lifetime of the caller-owned index vectors that produced
///   `sorted_leaf_indices`.
/// - `'updates` is the borrow of `updates`, used only during construction of the original skeleton
///   tree, but does not retain a reference into the output `OriginalSkeletonTreeImpl<'indices>`.
///
/// Unifying the lifetimes would make the returned `OriginalSkeletonForest<'indices>` to
/// borrow the storage update maps too, which would prevent `commit_block` from later moving those
/// maps by value into the compute phase.
struct TrieReadTask<'indices, 'updates, Layout: NodeLayoutFor<StarknetStorageValue>> {
    address: ContractAddress,
    updates: &'updates LeafModifications<StarknetStorageValue>,
    storage_root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'indices>,
    warn_on_trivial_modifications: bool,
    _layout: PhantomData<Layout>,
}

impl<'indices, 'updates, S, Layout> StorageTaskOutput<S>
    for TrieReadTask<'indices, 'updates, Layout>
where
    S: ImmutableReadOnlyStorage,
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
{
    type Output = ForestResult<(ContractAddress, OriginalSkeletonTreeImpl<'indices>)>;
}

#[async_trait]
impl<'indices, 'updates, 'storage, S, Layout> StorageTask<'storage, S>
    for TrieReadTask<'indices, 'updates, Layout>
where
    S: ImmutableReadOnlyStorage + 'storage,
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    async fn run_with_storage(
        self,
        storage: &mut ReadsCollectorStorage<'storage, S>,
    ) -> Self::Output {
        let skeleton = create_storage_trie::<Layout>(
            storage,
            self.address,
            self.updates,
            self.storage_root_hash,
            self.sorted_leaf_indices,
            self.warn_on_trivial_modifications,
        )
        .await?;
        Ok((self.address, skeleton))
    }
}

/// Storage task for fetching Patricia paths in a single storage trie.
struct StoragePathsReadTask<'indices, Layout: NodeLayoutFor<StarknetStorageValue>> {
    address: ContractAddress,
    storage_root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'indices>,
    _layout: PhantomData<Layout>,
}

impl<'indices, S, Layout> StorageTaskOutput<S> for StoragePathsReadTask<'indices, Layout>
where
    S: ImmutableReadOnlyStorage,
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
{
    type Output = TraversalResult<(ContractAddress, PreimageMap)>;
}

#[async_trait]
impl<'indices, 'storage, S, Layout> StorageTask<'storage, S>
    for StoragePathsReadTask<'indices, Layout>
where
    S: ImmutableReadOnlyStorage + 'storage,
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    async fn run_with_storage(
        self,
        storage: &mut ReadsCollectorStorage<'storage, S>,
    ) -> Self::Output {
        let leaves = None;
        let proof = fetch_patricia_paths::<Layout::DbLeaf, Layout>(
            storage,
            self.storage_root_hash,
            self.sorted_leaf_indices,
            leaves,
            &self.address,
        )
        .await?;
        Ok((self.address, proof))
    }
}

async fn create_storage_tries_concurrently<'a, S, Layout>(
    storage: &mut S,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    warn_on_trivial_modifications: bool,
    storage_tries_sorted_indices: &'a HashMap<ContractAddress, SortedLeafIndices<'a>>,
) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>>
where
    S: GatherableStorage,
    Layout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    let mut tasks = Vec::new();
    for (address, updates) in actual_storage_updates {
        tasks.push(TrieReadTask::<Layout> {
            address: *address,
            updates,
            storage_root_hash: original_contracts_trie_leaves
                .get(&contract_address_into_node_index(address))
                .ok_or(ForestError::MissingContractCurrentState(*address))?
                .storage_root_hash,
            sorted_leaf_indices: *storage_tries_sorted_indices
                .get(address)
                .ok_or(ForestError::MissingSortedLeafIndices(*address))?,
            warn_on_trivial_modifications,
            _layout: PhantomData,
        });
    }
    storage.gather(tasks).await.into_iter().collect()
}

/// Fetches Patricia proofs for the storage tries. If the storage has a [GatherableStorage] version,
/// then the paths are fetched concurrently. Otherwise, they are fetched sequentially.
pub(crate) async fn fetch_contract_storage_paths<StorageLayout, ContractLeaf>(
    storage: &mut impl ReadOnlyStorage,
    contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
    contract_leaves: &HashMap<NodeIndex, ContractLeaf>,
) -> TraversalResult<HashMap<ContractAddress, PreimageMap>>
where
    StorageLayout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <StorageLayout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
    ContractLeaf: AsRef<ContractState>,
{
    if let Some(gatherable_storage) = storage.as_gatherable_storage() {
        return fetch_contract_storage_paths_concurrently::<_, StorageLayout, ContractLeaf>(
            gatherable_storage,
            contract_storage_sorted_leaf_indices,
            contract_leaves,
        )
        .await;
    }
    fetch_contract_storage_paths_sequentially::<StorageLayout, ContractLeaf>(
        storage,
        contract_storage_sorted_leaf_indices,
        contract_leaves,
    )
    .await
}

/// Returns the contract address and storage root hash for the given leaf index, if the contract
/// exists in the contracts trie.
///
/// The contract address might not exist in the contracts trie in the following cases:
/// 1. We are looking at the previous tree and the contract is new.
/// 2. We are looking at the new tree and the contract is deleted (revert).
///
/// In either case, the storage trie of this contract is empty, so there is nothing to
/// prove regarding the contract storage.
pub(crate) fn get_address_and_storage_root<ContractLeaf: AsRef<ContractState>>(
    idx: &NodeIndex,
    contract_leaves: &HashMap<NodeIndex, ContractLeaf>,
) -> Option<(ContractAddress, HashOutput)> {
    let contract_address = try_node_index_into_contract_address(idx).unwrap_or_else(|_| {
        panic!(
            "Converting leaf NodeIndex to ContractAddress should succeed; failed to convert \
             {idx:?}."
        )
    });
    let storage_root_hash = contract_leaves.get(idx).map(|leaf| leaf.as_ref().storage_root_hash)?;
    Some((contract_address, storage_root_hash))
}

/// Sequentially fetches Patricia proofs for the storage tries.
async fn fetch_contract_storage_paths_sequentially<StorageLayout, ContractLeaf>(
    storage: &mut impl ReadOnlyStorage,
    contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
    contract_leaves: &HashMap<NodeIndex, ContractLeaf>,
) -> TraversalResult<HashMap<ContractAddress, PreimageMap>>
where
    StorageLayout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <StorageLayout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
    ContractLeaf: AsRef<ContractState>,
{
    let mut contracts_trie_storage_proofs =
        HashMap::with_capacity(contract_storage_sorted_leaf_indices.len());

    for (idx, sorted_leaf_indices) in contract_storage_sorted_leaf_indices {
        let Some((contract_address, storage_root_hash)) =
            get_address_and_storage_root(idx, contract_leaves)
        else {
            continue;
        };

        let leaves = None;
        let proof = fetch_patricia_paths::<StorageLayout::DbLeaf, StorageLayout>(
            storage,
            storage_root_hash,
            *sorted_leaf_indices,
            leaves,
            &contract_address,
        )
        .await?;
        contracts_trie_storage_proofs.insert(contract_address, proof);
    }

    Ok(contracts_trie_storage_proofs)
}

/// Concurrently fetches Patricia proofs for the storage tries.
async fn fetch_contract_storage_paths_concurrently<S, StorageLayout, ContractLeaf>(
    storage: &mut S,
    contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
    contract_leaves: &HashMap<NodeIndex, ContractLeaf>,
) -> TraversalResult<HashMap<ContractAddress, PreimageMap>>
where
    S: GatherableStorage,
    StorageLayout: NodeLayoutFor<StarknetStorageValue> + Send + 'static,
    <StorageLayout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
    ContractLeaf: AsRef<ContractState>,
{
    let mut tasks = Vec::new();
    for (idx, sorted_leaf_indices) in contract_storage_sorted_leaf_indices {
        let Some((contract_address, storage_root_hash)) =
            get_address_and_storage_root(idx, contract_leaves)
        else {
            continue;
        };
        tasks.push(StoragePathsReadTask::<StorageLayout> {
            address: contract_address,
            storage_root_hash,
            sorted_leaf_indices: *sorted_leaf_indices,
            _layout: PhantomData,
        });
    }
    storage.gather(tasks).await.into_iter().collect()
}

/// Helper function to create a storage trie for a single contract.
async fn create_storage_trie<'a, Layout: NodeLayoutFor<StarknetStorageValue>>(
    storage: &mut impl ReadOnlyStorage,
    address: ContractAddress,
    updates: &LeafModifications<StarknetStorageValue>,
    storage_root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    warn_on_trivial_modifications: bool,
) -> ForestResult<OriginalSkeletonTreeImpl<'a>>
where
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
{
    let trie_config =
        OriginalSkeletonTrieConfig::new_for_classes_or_storage_trie(warn_on_trivial_modifications);

    // TODO(Ariel): Change `LeafModifications` in `actual_storage_updates` to be an
    // iterator over borrowed data so that the conversion below is costless.
    let leaf_modifications: HashMap<
        NodeIndex,
        <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf,
    > = updates.iter().map(|(idx, value)| (*idx, Layout::DbLeaf::from(*value))).collect();

    let previous_leaves = None;
    Ok(create_original_skeleton_tree::<Layout::DbLeaf, Layout>(
        storage,
        storage_root_hash,
        sorted_leaf_indices,
        &trie_config,
        &leaf_modifications,
        previous_leaves,
        &address,
    )
    .await?)
}
