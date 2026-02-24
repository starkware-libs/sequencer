use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;

use ethnum::U256;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
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
use starknet_patricia::patricia_merkle_tree::traversal::{
    SubTreeTrait,
    TraversalResult,
    UnmodifiedChildTraversal,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::storage_trait::{DbKey, Storage};
use tracing::warn;

use crate::block_committer::input::{
    contract_address_into_node_index,
    ReaderConfig,
    StarknetStorageValue,
};
use crate::db::db_layout::DbLayout;
use crate::db::long_edge_cache::{
    compute_all_path_indices_with_cache,
    LongEdgeCache,
    StorageTriesLongEdgeCache,
    MIN_EDGE_LENGTH_FOR_CACHE,
};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::OriginalSkeletonTrieConfig;
use crate::patricia_merkle_tree::types::CompiledClassHash;

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
    storage: &mut impl Storage,
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
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.into_iter()) {
            if subtree.is_unmodified() {
                handle_unmodified_subtree(
                    skeleton_tree,
                    &mut next_subtrees,
                    filled_root,
                    subtree,
                    None,
                );
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
    long_edge_cache: Option<&mut LongEdgeCache>,
) where
    SubTree::NodeData: Clone,
{
    // Sanity check.
    assert!(subtree.is_unmodified(), "Called `handle_unmodified_subtree` for a modified subtree.");

    let root_index = subtree.get_root_index();

    match filled_root.data {
        NodeData::Edge(EdgeData { bottom_data, path_to_bottom }) => {
            if u8::from(path_to_bottom.length) >= MIN_EDGE_LENGTH_FOR_CACHE {
                if let Some(cache) = long_edge_cache {
                    let bottom_index = path_to_bottom.bottom_index(root_index);
                    cache.insert(bottom_index, root_index);
                }
            }
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
    storage: &mut impl Storage,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<Vec<FilledNode<L, Layout::NodeData>>> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> =
        subtrees.iter().map(|subtree| subtree.get_root_db_key::<L>(key_context)).collect();

    let db_vals = storage.mget(&db_keys.iter().collect::<Vec<&DbKey>>()).await?;
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
    storage: &mut impl Storage,
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

/// Computes all possible node indices on paths from root to the given leaf indices.
/// Returns a deduplicated vector of indices (sorted by index value).
///
/// For a tree of height 251, each leaf path has up to 251 ancestors.
/// Paths share prefixes, so deduplication significantly reduces the total count.
fn compute_all_path_indices(leaf_indices: &[NodeIndex]) -> Vec<NodeIndex> {
    let mut all_indices: Vec<NodeIndex> = Vec::new();

    for leaf_idx in leaf_indices {
        // Walk from leaf up to root, collecting all ancestors
        let mut current = *leaf_idx;
        while current.0 >= U256::ONE {
            all_indices.push(current);
            if current == NodeIndex::ROOT {
                break;
            }
            // Move to parent: parent_index = current_index / 2
            current = current >> 1;
        }
    }

    // Sort and deduplicate
    all_indices.sort();
    all_indices.dedup();
    all_indices
}

/// Creates an original skeleton tree using speculative reads.
///
/// Instead of iteratively fetching nodes level by level, this function:
/// 1. Computes ALL possible indices on paths from root to leaves
/// 2. Fetches them all in a single mget (bloom filters handle non-existent keys efficiently)
/// 3. Builds the skeleton tree using the existing fetch_nodes logic with preloaded data
///
/// This reduces round trips from O(tree_height) to O(1) at the cost of requesting
/// more keys (most of which will be filtered by bloom filters without disk I/O).
///
/// When `long_edge_cache` is provided, it is used to reduce the number of indices fetched
/// (skipping indices inside long edges) and is populated with any long edges (length >= 5)
/// discovered during the traversal.
pub async fn create_original_skeleton_tree_speculative<'a, L: Leaf, Layout: NodeLayout<'a, L>>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    config: &impl OriginalSkeletonTreeConfig,
    leaf_modifications: &LeafModifications<L>,
    previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    mut long_edge_cache: Option<&mut LongEdgeCache>,
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

    // Step 1: Compute all possible indices on paths from root to leaves (use cache if provided)
    let all_indices = match &long_edge_cache {
        Some(cache) => {
            compute_all_path_indices_with_cache(sorted_leaf_indices.get_indices(), cache)
        }
        None => compute_all_path_indices(sorted_leaf_indices.get_indices()),
    };

    // Step 2: Build subtrees for all indices to get their DB keys
    let subtrees: Vec<Layout::SubTree> = all_indices
        .iter()
        .map(|idx| {
            Layout::SubTree::create(
                sorted_leaf_indices,
                *idx,
                Layout::NodeData::from(root_hash.into()),
            )
        })
        .collect();

    // Step 3: Build DB keys for all subtrees
    let db_keys: Vec<DbKey> =
        subtrees.iter().map(|subtree| subtree.get_root_db_key::<L>(key_context)).collect();

    // Step 4: Single mget for all keys (bloom filters will filter non-existent keys)
    let db_key_refs: Vec<&DbKey> = db_keys.iter().collect();
    let db_values = storage.mget(&db_key_refs).await?;

    // Step 5: Build a map from NodeIndex to Option<DbValue>
    let mut value_map: HashMap<
        NodeIndex,
        Option<starknet_patricia_storage::storage_trait::DbValue>,
    > = HashMap::new();
    for (idx, opt_value) in all_indices.iter().zip(db_values.into_iter()) {
        value_map.insert(*idx, opt_value);
    }

    // Step 6: Build skeleton tree using the same traversal logic but with preloaded data.
    // Missing nodes (e.g. siblings not on the path from modified leaves) are fetched from storage.
    let main_subtree =
        Layout::SubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash.into());
    let mut skeleton_tree = OriginalSkeletonTreeImpl { nodes: HashMap::new(), sorted_leaf_indices };

    fetch_nodes_preloaded::<L, Layout, _>(
        &mut skeleton_tree,
        vec![main_subtree],
        &value_map,
        leaf_modifications,
        config,
        previous_leaves,
        key_context,
        &mut long_edge_cache,
        storage,
    )
    .await?;

    Ok(skeleton_tree)
}

/// Same as fetch_nodes but uses preloaded values first; falls back to storage for missing nodes
/// (e.g. siblings not on the path from modified leaves to root). When `long_edge_cache` is
/// provided, populates it with edge nodes of length >= 5.
async fn fetch_nodes_preloaded<'a, L, Layout, S>(
    skeleton_tree: &mut OriginalSkeletonTreeImpl<'a>,
    subtrees: Vec<Layout::SubTree>,
    preloaded_values: &HashMap<
        NodeIndex,
        Option<starknet_patricia_storage::storage_trait::DbValue>,
    >,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig,
    mut previous_leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    long_edge_cache: &mut Option<&mut LongEdgeCache>,
    storage: &mut S,
) -> OriginalSkeletonTreeResult<()>
where
    L: Leaf,
    Layout: NodeLayout<'a, L>,
    S: Storage,
{
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    let should_fetch_modified_leaves =
        config.compare_modified_leaves() || previous_leaves.is_some();

    while !current_subtrees.is_empty() {
        // Get roots from preloaded values, falling back to storage for missing nodes (e.g.
        // siblings)
        let filled_roots = get_roots_from_preloaded::<L, Layout, S>(
            &current_subtrees,
            preloaded_values,
            key_context,
            storage,
        )
        .await?;

        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.into_iter()) {
            if subtree.is_unmodified() {
                handle_unmodified_subtree(
                    skeleton_tree,
                    &mut next_subtrees,
                    filled_root,
                    subtree,
                    long_edge_cache.as_deref_mut(),
                );
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
                    if u8::from(path_to_bottom.length) >= MIN_EDGE_LENGTH_FOR_CACHE {
                        if let Some(cache) = long_edge_cache.as_deref_mut() {
                            let bottom_index = path_to_bottom.bottom_index(root_index);
                            cache.insert(bottom_index, root_index);
                        }
                    }
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

/// Gets roots from preloaded values, falling back to storage when a node is missing (e.g. a
/// sibling not on the path from modified leaves to root).
async fn get_roots_from_preloaded<'a, L: Leaf, Layout: NodeLayout<'a, L>, S: Storage>(
    subtrees: &[Layout::SubTree],
    preloaded_values: &HashMap<
        NodeIndex,
        Option<starknet_patricia_storage::storage_trait::DbValue>,
    >,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    storage: &mut S,
) -> TraversalResult<Vec<FilledNode<L, Layout::NodeData>>> {
    use starknet_patricia::patricia_merkle_tree::traversal::TraversalError;

    let mut subtrees_roots = vec![];

    for subtree in subtrees {
        let root_index = subtree.get_root_index();
        let db_key = subtree.get_root_db_key::<L>(key_context);

        let val = match preloaded_values.get(&root_index) {
            Some(Some(v)) => v.clone(),
            _ => {
                // Fallback to storage (e.g. sibling branch not in preloaded path)
                match storage.get(&db_key).await.map_err(TraversalError::from)? {
                    Some(v) => v,
                    None => return Err(StorageError::MissingKey(db_key).into()),
                }
            }
        };

        let filled_node =
            Layout::NodeDbObject::deserialize(&val, &subtree.get_root_context())?.into();
        subtrees_roots.push(filled_node);
    }
    Ok(subtrees_roots)
}

pub async fn create_storage_tries<'a, Layout: NodeLayoutFor<StarknetStorageValue> + DbLayout>(
    storage: &mut impl Storage,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    config: &ReaderConfig,
    storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
    storage_tries_long_edge_cache: &mut StorageTriesLongEdgeCache,
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
        let contract_state = original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .ok_or(ForestError::MissingContractCurrentState(*address))?;
        let config = OriginalSkeletonTrieConfig::new_for_classes_or_storage_trie(
            config.warn_on_trivial_modifications(),
        );
        let leaf_modifications =
            updates.iter().map(|(idx, value)| (*idx, Layout::DbLeaf::from(*value))).collect();
        let cache_for_address = storage_tries_long_edge_cache.entry(*address).or_default();

        let original_skeleton = if Layout::USE_SPECULATIVE_STORAGE_READ {
            // Speculative single mget + storage fallback for missing nodes (e.g. siblings).
            create_original_skeleton_tree_speculative::<Layout::DbLeaf, Layout>(
                storage,
                contract_state.storage_root_hash,
                *sorted_leaf_indices,
                &config,
                &leaf_modifications,
                None,
                address,
                Some(cache_for_address),
            )
            .await?
        } else {
            // Layout uses hash-based keys (e.g. Facts); must read layer-by-layer.
            let skeleton = create_original_skeleton_tree::<Layout::DbLeaf, Layout>(
                storage,
                contract_state.storage_root_hash,
                *sorted_leaf_indices,
                &config,
                &leaf_modifications,
                None,
                address,
            )
            .await?;
            for (idx, node) in &skeleton.nodes {
                if let OriginalSkeletonNode::Edge(path_to_bottom) = node {
                    if u8::from(path_to_bottom.length) >= MIN_EDGE_LENGTH_FOR_CACHE {
                        let bottom_index = path_to_bottom.bottom_index(*idx);
                        cache_for_address.insert(bottom_index, *idx);
                    }
                }
            }
            skeleton
        };
        storage_tries.insert(*address, original_skeleton);
    }
    Ok(storage_tries)
}

/// Creates the contracts trie original skeleton.
/// Also returns the previous contracts state of the modified contracts.
pub async fn create_contracts_trie<'a, Layout: NodeLayoutFor<ContractState>>(
    storage: &mut impl Storage,
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
    storage: &mut impl Storage,
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
