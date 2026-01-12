use std::collections::HashMap;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::Storage;

use crate::db::facts_db::FactsNodeLayout;
use crate::db::trie_traversal::create_original_skeleton_tree;
use crate::patricia_merkle_tree::tree::OriginalSkeletonTrieConfig;

#[cfg(test)]
#[path = "create_facts_tree_test.rs"]
pub mod create_facts_tree_test;

/// Prepares the OS inputs by fetching paths to the given leaves (i.e. their induced Skeleton tree).
/// Note that ATM, the Rust committer does not manage history and is not used for storage proofs;
/// Thus, this function assumes facts layout.
pub async fn get_leaves<'a, L: Leaf>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> OriginalSkeletonTreeResult<HashMap<NodeIndex, L>> {
    let config = OriginalSkeletonTrieConfig::default();
    let leaf_modifications = LeafModifications::new();
    let mut previous_leaves = HashMap::new();
    create_original_skeleton_tree::<L, FactsNodeLayout>(
        storage,
        root_hash,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
        Some(&mut previous_leaves),
        key_context,
    )
    .await?;
    Ok(previous_leaves)
}
