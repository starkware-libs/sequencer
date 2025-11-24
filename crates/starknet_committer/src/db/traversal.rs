use std::collections::HashMap;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    NodeData,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::{SubTree, TraversalResult};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, Storage};

#[cfg(test)]
#[path = "traversal_test.rs"]
pub mod traversal_test;

// TODO(Aviv, 17/07/2024): Split between storage prefix implementation and function logic.
pub fn calculate_subtrees_roots<'a, L: Leaf>(
    subtrees: &[SubTree<'a>],
    storage: &mut impl Storage,
) -> TraversalResult<Vec<FilledNode<L>>> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> = subtrees
        .iter()
        .map(|subtree| {
            create_db_key(subtree.get_root_prefix::<L>().into(), &subtree.root_hash.0.to_bytes_be())
        })
        .collect();

    let db_vals = storage.mget(&db_keys.iter().collect::<Vec<&DbKey>>())?;
    for ((subtree, optional_val), db_key) in subtrees.iter().zip(db_vals.iter()).zip(db_keys) {
        let Some(val) = optional_val else { Err(StorageError::MissingKey(db_key))? };
        subtrees_roots.push(FilledNode::deserialize(subtree.root_hash, val, subtree.is_leaf())?)
    }
    Ok(subtrees_roots)
}

/// Returns the Patricia inner nodes ([PreimageMap]) in the paths to the given `leaf_indices` in the
/// given tree according to the `root_hash`.
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
pub fn fetch_patricia_paths<L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'_>,
    leaves: Option<&mut HashMap<NodeIndex, L>>,
) -> TraversalResult<PreimageMap> {
    let mut witnesses = PreimageMap::new();

    if sorted_leaf_indices.is_empty() || root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok(witnesses);
    }

    let main_subtree = SubTree { sorted_leaf_indices, root_index: NodeIndex::ROOT, root_hash };

    fetch_patricia_paths_inner::<L>(storage, vec![main_subtree], &mut witnesses, leaves)?;
    Ok(witnesses)
}

/// Fetches the inner nodes [HashOutput] and [Preimage] in the paths to modified leaves.
/// The siblings (witnesses) are included in the [Preimage] of their parent node.
/// Required for `patricia_update` function in Cairo.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// inner nodes in their paths.
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
fn fetch_patricia_paths_inner<'a, L: Leaf>(
    storage: &mut impl Storage,
    subtrees: Vec<SubTree<'a>>,
    witnesses: &mut PreimageMap,
    mut leaves: Option<&mut HashMap<NodeIndex, L>>,
) -> TraversalResult<()> {
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    while !current_subtrees.is_empty() {
        let filled_roots = calculate_subtrees_roots::<L>(&current_subtrees, storage)?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.iter()) {
            // Always insert root.
            // No need to insert an unmodified node (which is not the root), because its parent is
            // inserted, and contains the preimage.
            if subtree.is_unmodified() {
                continue;
            }
            match filled_root.data {
                // Binary node.
                NodeData::Binary(binary_data) => {
                    witnesses.insert(subtree.root_hash, Preimage::Binary(binary_data.clone()));
                    let (left_subtree, right_subtree) = subtree
                        .get_children_subtrees(binary_data.left_hash, binary_data.right_hash);
                    next_subtrees.push(left_subtree);
                    next_subtrees.push(right_subtree);
                }
                // Edge node.
                NodeData::Edge(edge_data) => {
                    witnesses.insert(subtree.root_hash, Preimage::Edge(edge_data));
                    // Parse bottom.
                    let (bottom_subtree, empty_leaves_indices) = subtree
                        .get_bottom_subtree(&edge_data.path_to_bottom, edge_data.bottom_hash);
                    if let Some(ref mut leaves_map) = leaves {
                        // Insert empty leaves descendent of the current subtree, that are not
                        // descendents of the bottom node.
                        for index in empty_leaves_indices {
                            leaves_map.insert(*index, L::default());
                        }
                    }
                    next_subtrees.push(bottom_subtree);
                }
                // Leaf node.
                NodeData::Leaf(leaf_data) => {
                    // Fetch the leaf if it's modified and should be fetched.
                    if let Some(ref mut leaves_map) = leaves {
                        leaves_map.insert(subtree.root_index, leaf_data);
                    }
                }
            }
        }
        current_subtrees = next_subtrees;
        next_subtrees = Vec::new();
    }
    Ok(())
}
