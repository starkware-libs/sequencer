use std::collections::HashMap;

use ethnum::U256;
use rand::Rng;
use serde_json::json;

use super::filled_tree::tree::{FilledTree, StorageTrie};
use super::node_data::leaf::{Leaf, LeafModifications, SkeletonLeaf};
use super::original_skeleton_tree::config::OriginalSkeletonStorageTrieConfig;
use super::original_skeleton_tree::tree::{OriginalSkeletonTree, OriginalSkeletonTreeImpl};
use super::types::{NodeIndex, SortedLeafIndices};
use super::updated_skeleton_tree::hash_function::TreeHashFunctionImpl;
use super::updated_skeleton_tree::tree::{UpdatedSkeletonTree, UpdatedSkeletonTreeImpl};
use crate::block_committer::input::StarknetStorageValue;
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::TypesError;
use crate::storage::map_storage::MapStorage;

impl TryFrom<&U256> for Felt {
    type Error = TypesError<U256>;
    fn try_from(value: &U256) -> Result<Self, Self::Error> {
        if *value > U256::from(&Felt::MAX) {
            return Err(TypesError::ConversionError {
                from: *value,
                to: "Felt",
                reason: "value is bigger than felt::max",
            });
        }
        Ok(Self::from_bytes_be(&value.to_be_bytes()))
    }
}

/// Generates a random U256 number between low and high (exclusive).
/// Panics if low > high
pub fn get_random_u256<R: Rng>(rng: &mut R, low: U256, high: U256) -> U256 {
    assert!(low < high);
    let high_of_low = low.high();
    let high_of_high = high.high();

    let delta = high - low;
    if delta <= u128::MAX {
        let delta = u128::try_from(delta).expect("Failed to convert delta to u128");
        return low + rng.gen_range(0..delta);
    }

    // Randomize the high 128 bits in the extracted range, and the low 128 bits in their entire
    // domain until the result is in range.
    // As high-low>u128::MAX, the expected number of samples until the loops breaks is bound from
    // above by 3 (as either:
    //  1. high_of_high > high_of_low + 1, and there is a 1/3 chance to get a valid result for high
    //  bits in (high_of_low, high_of_high).
    //  2. high_of_high == high_of_low + 1, and every possible low 128 bits value is valid either
    // when the high bits equal high_of_high, or when they equal high_of_low).
    let mut randomize = || {
        U256::from_words(rng.gen_range(*high_of_low..=*high_of_high), rng.gen_range(0..=u128::MAX))
    };
    let mut result = randomize();
    while result < low || result >= high {
        result = randomize();
    }
    result
}

pub async fn tree_computation_flow(
    leaf_modifications: LeafModifications<StarknetStorageValue>,
    storage: &MapStorage,
    root_hash: HashOutput,
) -> StorageTrie {
    let config = OriginalSkeletonStorageTrieConfig::new(&leaf_modifications, false);
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);
    let mut original_skeleton =
        OriginalSkeletonTreeImpl::create(storage, root_hash, sorted_leaf_indices, &config)
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

    StorageTrie::create_with_existing_leaves::<TreeHashFunctionImpl>(
        updated_skeleton.into(),
        leaf_modifications,
    )
    .await
    .expect("Failed to create the filled tree")
}

pub async fn single_tree_flow_test(
    leaf_modifications: LeafModifications<StarknetStorageValue>,
    storage: MapStorage,
    root_hash: HashOutput,
) -> String {
    // Move from leaf number to actual index.
    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<StarknetStorageValue>>();

    let filled_tree = tree_computation_flow(leaf_modifications, &storage, root_hash).await;

    let hash_result = filled_tree.get_root_hash();

    let mut result_map = HashMap::new();
    // Serialize the hash result.
    let json_hash = &json!(hash_result.0.to_hex());
    result_map.insert("root_hash", json_hash);
    // Serlialize the storage modifications.
    let json_storage = &json!(filled_tree.serialize());
    result_map.insert("storage_changes", json_storage);
    serde_json::to_string(&result_map).expect("serialization failed")
}
