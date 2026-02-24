//! Cache of long edge nodes (edges with length >= 5) for speculative skeleton tree building.
//!
//! The cache maps bottom node index -> edge root node index. When walking from leaf toward root
//! to compute path indices, we look up the current index; if it's a cached bottom we jump to
//! its edge root and skip the intermediate indices.

use std::collections::{HashMap, HashSet};

use ethnum::U256;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::HashFilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;

use crate::forest::deleted_nodes::DeletedNodes;
use crate::forest::filled_forest::FilledForest;

/// Minimum edge path length to store in the cache. Edges with length >= this are cached.
pub const MIN_EDGE_LENGTH_FOR_CACHE: u8 = 5;

/// Maps bottom node index to edge root node index for long edges (length >= 5).
/// Stored in this direction so path computation can look up by current index (a bottom) directly.
pub type LongEdgeCache = HashMap<NodeIndex, NodeIndex>;

/// Per-contract cache for storage tries long edges. Used in the main read/write flow.
pub type StorageTriesLongEdgeCache = HashMap<ContractAddress, LongEdgeCache>;

/// Computes all path indices without using the cache (every ancestor from leaf to root).
fn compute_all_path_indices_no_cache(leaf_indices: &[NodeIndex]) -> Vec<NodeIndex> {
    let mut all_indices: Vec<NodeIndex> = Vec::new();
    for leaf_idx in leaf_indices {
        let mut current = *leaf_idx;
        while current.0 >= U256::ONE {
            all_indices.push(current);
            if current == NodeIndex::ROOT {
                break;
            }
            current = current >> 1;
        }
    }
    all_indices.sort();
    all_indices.dedup();
    all_indices
}

/// Computes all node indices on paths from root to the given leaf indices, using the cache
/// to skip indices that lie inside long edges. When walking from leaf toward root, if the
/// current index is the bottom of a cached edge, we look it up and jump to the edge root.
///
/// Returns a deduplicated vector of indices (sorted by index value).
pub fn compute_all_path_indices_with_cache(
    leaf_indices: &[NodeIndex],
    cache: &LongEdgeCache,
) -> Vec<NodeIndex> {
    if cache.is_empty() {
        return compute_all_path_indices_no_cache(leaf_indices);
    }
    let mut all_indices: Vec<NodeIndex> = Vec::new();

    for leaf_idx in leaf_indices {
        let mut current = *leaf_idx;
        while current.0 >= U256::ONE {
            all_indices.push(current);
            if current == NodeIndex::ROOT {
                break;
            }
            if let Some(&edge_root) = cache.get(&current) {
                current = edge_root;
            } else {
                current = current >> 1;
            }
        }
    }

    all_indices.sort();
    all_indices.dedup();
    all_indices
}

/// Removes from the cache all entries whose key (bottom) or value (edge root) is in `deleted`.
pub fn remove_deleted_from_cache(cache: &mut LongEdgeCache, deleted: &HashSet<NodeIndex>) {
    if deleted.is_empty() {
        return;
    }
    cache.retain(|k, v| !deleted.contains(k) && !deleted.contains(v));
}

/// Inserts long edges from a filled tree into the cache (bottom -> edge root). Call after
/// removing deleted nodes so that new edges from the written tree are reflected.
pub fn insert_long_edges_from_filled_tree<
    L: starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf,
>(
    cache: &mut LongEdgeCache,
    tree_map: &std::collections::HashMap<NodeIndex, HashFilledNode<L>>,
) {
    for (index, node) in tree_map {
        if let NodeData::Edge(edge_data) = &node.data {
            if u8::from(edge_data.path_to_bottom.length) >= MIN_EDGE_LENGTH_FOR_CACHE {
                let bottom_index = edge_data.path_to_bottom.bottom_index(*index);
                cache.insert(bottom_index, *index);
            }
        }
    }
}

/// Updates the cache after a trie write: removes entries for deleted nodes (as edge root or
/// bottom), then adds long edges from the new filled tree.
pub fn update_long_edge_cache_after_write<
    L: starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf,
>(
    cache: &mut LongEdgeCache,
    tree_map: &std::collections::HashMap<NodeIndex, HashFilledNode<L>>,
    deleted: &HashSet<NodeIndex>,
) {
    remove_deleted_from_cache(cache, deleted);
    insert_long_edges_from_filled_tree(cache, tree_map);
}

/// Updates all storage-tries long-edge caches after a forest write. Call from
/// `write_with_metadata` so the cache reflects the new state and removed edge nodes.
pub fn update_storage_tries_long_edge_caches(
    caches: &mut StorageTriesLongEdgeCache,
    filled_forest: &FilledForest,
    deleted_nodes: &DeletedNodes,
) {
    let empty = HashSet::new();
    for (address, tree) in &filled_forest.storage_tries {
        let deleted = deleted_nodes.storage_tries.get(address).unwrap_or(&empty);
        let cache = caches.entry(*address).or_default();
        update_long_edge_cache_after_write(cache, &tree.tree_map, deleted);
    }
}
