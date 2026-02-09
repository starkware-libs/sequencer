use std::collections::{HashMap, HashSet};

use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;

use crate::block_committer::input::StarknetStorageValue;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::OriginalSkeletonForest;
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Holds deleted node indices organized by tree type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedNodes {
    pub classes_trie: HashSet<NodeIndex>,
    pub contracts_trie: HashSet<NodeIndex>,
    pub storage_tries: HashMap<ContractAddress, HashSet<NodeIndex>>,
}

impl DeletedNodes {
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.classes_trie.len()
            + self.contracts_trie.len()
            + self.storage_tries.values().map(|leaves| leaves.len()).sum::<usize>()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.classes_trie.is_empty()
            && self.contracts_trie.is_empty()
            && self.storage_tries.values().all(|leaves| leaves.is_empty())
    }
}

/// Returns the deleted leaves, and inner nodes that are:
/// 1. In the original tree
/// 2. Not of type OriginalSkeletonNode::UnmodifiedSubTree
/// 3. Not in the updated tree
fn find_deleted_nodes_for_tree<'a>(
    deleted_leaves_indices: HashSet<NodeIndex>,
    original_skeleton_tree: &impl OriginalSkeletonTree<'a>,
    updated_skeleton_tree: &impl UpdatedSkeletonTree<'a>,
) -> HashSet<NodeIndex> {
    let mut deleted_nodes_indices = deleted_leaves_indices;

    // Iterate through all nodes in the original tree
    for (node_index, node) in original_skeleton_tree.get_nodes() {
        // Skip UnmodifiedSubTree nodes
        if matches!(node, OriginalSkeletonNode::UnmodifiedSubTree(_)) {
            continue;
        }

        // If node does not exist in updated tree, it is deleted
        if updated_skeleton_tree.get_node(*node_index).is_err() {
            deleted_nodes_indices.insert(*node_index);
        }
    }

    deleted_nodes_indices
}

/// Finds all deleted nodes across all trees in the forest.
/// Compares the original skeleton forest with the updated skeleton forest.
pub(crate) fn find_deleted_nodes(
    original_forest: &OriginalSkeletonForest<'_>,
    updated_forest: &UpdatedSkeletonForest,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    actual_classes_updates: &LeafModifications<CompiledClassHash>,
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
) -> ForestResult<DeletedNodes> {
    // Find all deleted nodes for classes_trie
    let classes_trie_deleted_leaves: HashSet<NodeIndex> = actual_classes_updates
        .iter()
        .filter_map(|(index, leaf)| if leaf.is_empty() { Some(*index) } else { None })
        .collect();
    let classes_trie_deleted_nodes = find_deleted_nodes_for_tree(
        classes_trie_deleted_leaves,
        &original_forest.classes_trie,
        &updated_forest.classes_trie,
    );

    // Find all deleted nodes for contracts_trie
    let contracts_trie_deleted_leaves = original_contracts_trie_leaves
        .iter()
        .filter_map(|(index, state)| {
            if !state.is_empty() && updated_forest.contracts_trie.get_node(*index).is_err() {
                Some(*index)
            } else {
                None
            }
        })
        .collect();
    let contracts_trie_deleted_nodes = find_deleted_nodes_for_tree(
        contracts_trie_deleted_leaves,
        &original_forest.contracts_trie,
        &updated_forest.contracts_trie,
    );

    // Find all deleted nodes for each storage_trie
    let mut storage_tries_deleted_leaves: HashMap<ContractAddress, HashSet<NodeIndex>> =
        HashMap::new();
    for (address, updates) in actual_storage_updates {
        let deleted_indices: HashSet<NodeIndex> = updates
            .iter()
            .filter_map(|(index, leaf)| if leaf.is_empty() { Some(*index) } else { None })
            .collect();
        if !deleted_indices.is_empty() {
            storage_tries_deleted_leaves.insert(*address, deleted_indices);
        }
    }
    let mut storage_tries_deleted_nodes: HashMap<ContractAddress, HashSet<NodeIndex>> =
        HashMap::new();
    for (address, deleted_leaves) in storage_tries_deleted_leaves {
        let original_storage_trie = original_forest
            .storage_tries
            .get(&address)
            .expect("Storage trie should exist for contract address with deleted leaves");
        let updated_storage_trie = updated_forest
            .storage_tries
            .get(&address)
            .expect("Updated storage trie should exist where the original trie exists");
        let deleted_nodes = find_deleted_nodes_for_tree(
            deleted_leaves,
            original_storage_trie,
            updated_storage_trie,
        );
        storage_tries_deleted_nodes.insert(address, deleted_nodes);
    }

    Ok(DeletedNodes {
        classes_trie: classes_trie_deleted_nodes,
        contracts_trie: contracts_trie_deleted_nodes,
        storage_tries: storage_tries_deleted_nodes,
    })
}
