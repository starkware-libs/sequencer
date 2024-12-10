use std::collections::HashMap;

use ethnum::U256;
use rand::Rng;
use serde_json::json;

use super::filled_tree::tree::{FilledTree, FilledTreeImpl};
use super::node_data::inner_node::{EdgePathLength, PathToBottom};
use super::node_data::leaf::{Leaf, LeafModifications, SkeletonLeaf};
use super::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use super::original_skeleton_tree::node::OriginalSkeletonNode;
use super::original_skeleton_tree::tree::{OriginalSkeletonTree, OriginalSkeletonTreeImpl};
use super::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use super::updated_skeleton_tree::hash_function::TreeHashFunction;
use super::updated_skeleton_tree::tree::{UpdatedSkeletonTree, UpdatedSkeletonTreeImpl};
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::TypesError;
use crate::storage::map_storage::MapStorage;
use crate::storage::storage_trait::{create_db_key, StarknetPrefix, StorageKey, StorageValue};

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

pub async fn tree_computation_flow<L, TH>(
    leaf_modifications: LeafModifications<L>,
    storage: &MapStorage,
    root_hash: HashOutput,
    config: impl OriginalSkeletonTreeConfig<L>,
) -> FilledTreeImpl<L>
where
    TH: TreeHashFunction<L> + 'static,
    L: Leaf + 'static,
{
    let mut sorted_leaf_indices: Vec<NodeIndex> = leaf_modifications.keys().copied().collect();
    let sorted_leaf_indices = SortedLeafIndices::new(&mut sorted_leaf_indices);
    let mut original_skeleton = OriginalSkeletonTreeImpl::create(
        storage,
        root_hash,
        sorted_leaf_indices,
        &config,
        &leaf_modifications,
    )
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
    storage: MapStorage,
    root_hash: HashOutput,
    config: impl OriginalSkeletonTreeConfig<L>,
) -> String {
    // Move from leaf number to actual index.
    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<L>>();

    let filled_tree =
        tree_computation_flow::<L, TH>(leaf_modifications, &storage, root_hash, config).await;

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

pub fn create_32_bytes_entry(simple_val: u128) -> [u8; 32] {
    U256::from(simple_val).to_be_bytes()
}

fn create_patricia_key(val: u128) -> StorageKey {
    create_db_key(StarknetPrefix::InnerNode.to_storage_prefix(), &U256::from(val).to_be_bytes())
}

fn create_binary_val(left: u128, right: u128) -> StorageValue {
    StorageValue(
        (create_32_bytes_entry(left).into_iter().chain(create_32_bytes_entry(right))).collect(),
    )
}

fn create_edge_val(hash: u128, path: u128, length: u8) -> StorageValue {
    StorageValue(
        create_32_bytes_entry(hash)
            .into_iter()
            .chain(create_32_bytes_entry(path))
            .chain([length])
            .collect(),
    )
}

pub fn create_binary_entry(left: u128, right: u128) -> (StorageKey, StorageValue) {
    (create_patricia_key(left + right), create_binary_val(left, right))
}

pub fn create_edge_entry(hash: u128, path: u128, length: u8) -> (StorageKey, StorageValue) {
    (create_patricia_key(hash + path + u128::from(length)), create_edge_val(hash, path, length))
}

pub fn create_binary_skeleton_node(idx: u128) -> (NodeIndex, OriginalSkeletonNode) {
    (NodeIndex::from(idx), OriginalSkeletonNode::Binary)
}

pub fn create_edge_skeleton_node(
    idx: u128,
    path: u128,
    length: u8,
) -> (NodeIndex, OriginalSkeletonNode) {
    (
        NodeIndex::from(idx),
        OriginalSkeletonNode::Edge(
            PathToBottom::new(path.into(), EdgePathLength::new(length).unwrap()).unwrap(),
        ),
    )
}

pub fn create_unmodified_subtree_skeleton_node(
    idx: u128,
    hash_output: u128,
) -> (NodeIndex, OriginalSkeletonNode) {
    (
        NodeIndex::from(idx),
        OriginalSkeletonNode::UnmodifiedSubTree(HashOutput(Felt::from(hash_output))),
    )
}

pub fn create_root_edge_entry(
    old_root: u128,
    subtree_height: SubTreeHeight,
) -> (StorageKey, StorageValue) {
    // Assumes path is 0.
    let length = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
    let new_root = old_root + u128::from(length);
    let key = create_db_key(
        StarknetPrefix::InnerNode.to_storage_prefix(),
        &Felt::from(new_root).to_bytes_be(),
    );
    let value = StorageValue(
        Felt::from(old_root)
            .to_bytes_be()
            .into_iter()
            .chain(Felt::ZERO.to_bytes_be())
            .chain([length])
            .collect(),
    );
    (key, value)
}

pub fn create_expected_skeleton_nodes(
    nodes: Vec<(NodeIndex, OriginalSkeletonNode)>,
    height: u8,
) -> HashMap<NodeIndex, OriginalSkeletonNode> {
    let subtree_height = SubTreeHeight::new(height);
    nodes
        .into_iter()
        .map(|(node_index, node)| (NodeIndex::from_subtree_index(node_index, subtree_height), node))
        .chain([(
            NodeIndex::ROOT,
            OriginalSkeletonNode::Edge(
                PathToBottom::new(0.into(), EdgePathLength::new(251 - height).unwrap()).unwrap(),
            ),
        )])
        .collect()
}

impl NodeIndex {
    /// Assumes self represents an index in a smaller tree height. Returns a node index represents
    /// the same index in the starknet state tree as if the smaller tree was 'planted' at the lowest
    /// leftmost node from the root.
    pub fn from_subtree_index(subtree_index: Self, subtree_height: SubTreeHeight) -> Self {
        let height_diff = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
        let offset = (NodeIndex::ROOT << height_diff) - 1.into();
        subtree_index + (offset << (subtree_index.bit_length() - 1))
    }
}
