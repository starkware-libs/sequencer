use std::collections::HashMap;

use serde_json::json;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{
    Leaf,
    LeafModifications,
    SkeletonLeaf,
};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::{
    UpdatedSkeletonTree,
    UpdatedSkeletonTreeImpl,
};
use starknet_patricia_storage::map_storage::MapStorage;

use crate::db::create_facts_tree::create_original_skeleton_tree;

pub async fn tree_computation_flow<L, TH>(
    leaf_modifications: LeafModifications<L>,
    storage: &mut MapStorage,
    root_hash: HashOutput,
    config: impl OriginalSkeletonTreeConfig<L>,
) -> FilledTreeImpl<L>
where
    TH: TreeHashFunction<L> + 'static,
    L: Leaf + 'static,
{
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);
    let mut original_skeleton = create_original_skeleton_tree(
        storage,
        root_hash,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
    .await
    .expect("Failed to create the original skeleton tree");

    let updated_skeleton: UpdatedSkeletonTreeImpl = UpdatedSkeletonTree::create(
        &mut original_skeleton,
        &leaf_modifications
            .iter()
            .map(|(index, data)| {
                (
                    *index,
                    match data.is_empty() {
                        true => SkeletonLeaf::Zero,
                        false => SkeletonLeaf::NonZero,
                    },
                )
            })
            .collect(),
    )
    .expect("Failed to create the updated skeleton tree");

    FilledTreeImpl::<L>::create_with_existing_leaves::<TH>(updated_skeleton, leaf_modifications)
        .await
        .expect("Failed to create the filled tree")
}

pub async fn single_tree_flow_test<L: Leaf + 'static, TH: TreeHashFunction<L> + 'static>(
    leaf_modifications: LeafModifications<L>,
    storage: &mut MapStorage,
    root_hash: HashOutput,
    config: impl OriginalSkeletonTreeConfig<L>,
) -> String {
    // Move from leaf number to actual index.
    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<L>>();

    let filled_tree =
        tree_computation_flow::<L, TH>(leaf_modifications, storage, root_hash, config).await;

    let hash_result = filled_tree.get_root_hash();

    let mut result_map = HashMap::new();
    // Serialize the hash result.
    let json_hash = &json!(hash_result.0.to_hex_string());
    result_map.insert("root_hash", json_hash);
    // Serlialize the storage modifications.
    let json_storage = &json!(filled_tree.serialize());
    result_map.insert("storage_changes", json_storage);
    serde_json::to_string(&result_map).expect("serialization failed")
}
