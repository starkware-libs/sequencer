use std::collections::HashMap;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    NodeData,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::traversal::{SubTreeTrait, TraversalResult};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::Storage;

use crate::db::facts_db::db::FactsNodeLayout;
use crate::db::facts_db::types::FactsSubTree;
use crate::db::trie_traversal::get_roots_from_storage;

#[cfg(test)]
#[path = "traversal_test.rs"]
pub mod traversal_test;

/// Returns the Patricia inner nodes ([PreimageMap]) in the paths to the given `leaf_indices` in the
/// given tree according to the `root_hash`.
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
pub async fn fetch_patricia_paths<L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'_>,
    leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<PreimageMap> {
    let mut witnesses = PreimageMap::new();

    if sorted_leaf_indices.is_empty() || root_hash == HashOutput::ROOT_OF_EMPTY_TREE {
        return Ok(witnesses);
    }

    let main_subtree = FactsSubTree::create(sorted_leaf_indices, NodeIndex::ROOT, root_hash);

    fetch_patricia_paths_inner::<L>(
        storage,
        vec![main_subtree],
        &mut witnesses,
        leaves,
        key_context,
    )
    .await?;
    Ok(witnesses)
}

// TODO(Rotem): Match the python logic of fetching the nodes.
/// Fetches the inner nodes [HashOutput] and [Preimage] in the paths to modified leaves.
/// Required for `patricia_update` function in Cairo.
/// Extra preimages (more than the data required to verify merkle paths) are required to verify
/// correctness of final tree topology; for more details see 'traverse_edge' in 'patricia.cairo'.
/// Given a list of subtrees, traverses towards their leaves and fetches all non-empty,
/// inner nodes in their paths and their siblings.
/// If `leaves` is not `None`, it also fetches the modified leaves and inserts them into the
/// provided map.
pub(crate) async fn fetch_patricia_paths_inner<'a, L: Leaf>(
    storage: &mut impl Storage,
    subtrees: Vec<FactsSubTree<'a>>,
    witnesses: &mut PreimageMap,
    mut leaves: Option<&mut HashMap<NodeIndex, L>>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> TraversalResult<()> {
    let mut current_subtrees = subtrees;
    let mut next_subtrees = Vec::new();
    while !current_subtrees.is_empty() {
        let filled_roots =
            get_roots_from_storage::<L, FactsNodeLayout>(&current_subtrees, storage, key_context)
                .await?;
        for (filled_root, subtree) in filled_roots.into_iter().zip(current_subtrees.iter()) {
            match filled_root.data {
                // Binary node.
                // If it's the root: It's a modified subtree.
                // Otherwise:
                // If it was inserted as a child of a binary node - it's a modified subtree.
                // If it was inserted as a child of an edge node - it should be fetched anyway
                // (modified or unmodified).
                NodeData::Binary(binary_data) => {
                    witnesses.insert(subtree.root_hash, Preimage::Binary(binary_data.clone()));
                    let (left_subtree, right_subtree) = subtree
                        .get_children_subtrees(binary_data.left_data, binary_data.right_data);

                    if !left_subtree.is_unmodified() {
                        next_subtrees.push(left_subtree);
                    }
                    if !right_subtree.is_unmodified() {
                        next_subtrees.push(right_subtree);
                    }
                }
                // Edge node.
                // If it's the root: it's not necessarily a modified tree, because the modification
                // might be a deletion. In this case, we want to fetch the bottom node just if it's
                // a binary node (and not a leaf). Otherwise: It was inserted as a child of a binary
                // node, so it's a modified subtree.
                NodeData::Edge(edge_data) => {
                    witnesses.insert(subtree.root_hash, Preimage::Edge(edge_data));
                    // Parse bottom.
                    let (bottom_subtree, empty_leaves_indices) = subtree
                        .get_bottom_subtree(&edge_data.path_to_bottom, edge_data.bottom_data);
                    if let Some(ref mut leaves_map) = leaves {
                        // Insert empty leaves descendent of the current subtree, that are not
                        // descendents of the bottom node.
                        for index in empty_leaves_indices {
                            leaves_map.insert(*index, L::default());
                        }
                    }
                    // Insert the bottom subtree if it's modified or a binary node.
                    if !bottom_subtree.is_unmodified() || !bottom_subtree.is_leaf() {
                        next_subtrees.push(bottom_subtree);
                    }
                }
                // Leaf node.
                // If it was inserted as a child of a binary node - it's a modified leaf.
                // If it was inserted as a child of an edge node - it means the edge node is
                // modified, meaning the leaf is also modified.
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
