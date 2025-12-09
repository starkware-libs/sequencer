use std::collections::HashMap;

use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::traversal::SubTreeTrait;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::db_object::{EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::storage_trait::Storage;

use crate::db::facts_db::db::FactsNodeLayout;
use crate::db::facts_db::types::FactsSubTree;
use crate::db::trie_traversal::fetch_nodes;
use crate::patricia_merkle_tree::tree::OriginalSkeletonTrieDontCompareConfig;

#[cfg(test)]
#[path = "create_facts_tree_test.rs"]
pub mod create_facts_tree_test;

pub async fn create_original_skeleton_tree_and_get_previous_leaves<
    'a,
    L: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>,
>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig,
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
        &EmptyKeyContext,
    )
    .await?;
    Ok((skeleton_tree, leaves))
}

/// Prepares the OS inputs by fetching paths to the given leaves (i.e. their induced Skeleton tree).
/// Note that ATM, the Rust committer does not manage history and is not used for storage proofs;
/// Thus, this function assumes facts layout.
pub async fn get_leaves<'a, L: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>>(
    storage: &mut impl Storage,
    root_hash: HashOutput,
    sorted_leaf_indices: SortedLeafIndices<'a>,
) -> OriginalSkeletonTreeResult<HashMap<NodeIndex, L>> {
    let config = OriginalSkeletonTrieDontCompareConfig;
    let leaf_modifications = LeafModifications::new();
    let (_, previous_leaves) = create_original_skeleton_tree_and_get_previous_leaves(
        storage,
        root_hash,
        sorted_leaf_indices,
        &leaf_modifications,
        &config,
    )
    .await?;
    Ok(previous_leaves)
}
