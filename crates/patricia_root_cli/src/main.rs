//! Reads a JSON object mapping storage keys to values (both hex felt strings) and
//! prints the Patricia root of the corresponding tree.
//!
//! Usage: `patricia_root_cli <path-to-json>`

use std::collections::HashMap;
use std::process::exit;
use std::{env, fs};

use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::db::facts_db::FactsNodeLayout;
use starknet_committer::db::trie_traversal::create_original_skeleton_tree;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::tree::OriginalSkeletonTrieConfig;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{
    Leaf,
    LeafModifications,
    SkeletonLeaf,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

#[tokio::main]
async fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: patricia_root_cli <path-to-json>");
        exit(1);
    });

    let contents = fs::read_to_string(&path).expect("Failed to read input file");
    let raw_entries: HashMap<Felt, Felt> =
        serde_json::from_str(&contents).expect("Failed to parse JSON as a felt->felt map");

    // Map each storage key to its leaf index in the trie, and each value to a storage leaf.
    let leaf_modifications: LeafModifications<StarknetStorageValue> = raw_entries
        .into_iter()
        .map(|(key, value)| (NodeIndex::from_leaf_felt(&key), StarknetStorageValue(value)))
        .collect();

    let root_hash = compute_patricia_root(leaf_modifications).await;
    println!("{}", root_hash.0.to_hex_string());
}

/// Builds a storage trie from `leaf_modifications` on top of an empty tree and returns its root.
///
/// Reuses the committer's three-step single-tree flow: original skeleton -> updated skeleton ->
/// filled tree.
async fn compute_patricia_root(
    leaf_modifications: LeafModifications<StarknetStorageValue>,
) -> HashOutput {
    let mut storage = MapStorage::default();
    // The contract address only affects DB key prefixes, not the resulting root hash.
    let key_context = ContractAddress::from(0_u128);
    let config = OriginalSkeletonTrieConfig::new_for_classes_or_storage_trie(false);

    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);

    let original_skeleton = create_original_skeleton_tree::<StarknetStorageValue, FactsNodeLayout>(
        &mut storage,
        HashOutput::ROOT_OF_EMPTY_TREE,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
        None,
        &key_context,
    )
    .await
    .expect("Failed to create the original skeleton tree");

    let leaf_skeleton_modifications = leaf_modifications
        .iter()
        .map(|(index, leaf)| {
            let skeleton_leaf =
                if leaf.is_empty() { SkeletonLeaf::Zero } else { SkeletonLeaf::NonZero };
            (*index, skeleton_leaf)
        })
        .collect();
    let updated_skeleton: UpdatedSkeletonTreeImpl =
        UpdatedSkeletonTree::create(&original_skeleton, &leaf_skeleton_modifications)
            .expect("Failed to create the updated skeleton tree");

    let filled_tree = FilledTreeImpl::create_with_existing_leaves::<TreeHashFunctionImpl>(
        updated_skeleton,
        leaf_modifications,
    )
    .await
    .expect("Failed to create the filled tree");

    filled_tree.get_root_hash()
}
