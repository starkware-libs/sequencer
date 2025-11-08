use ethnum::U256;
use rand::rngs::ThreadRng;
use rstest::{fixture, rstest};
use starknet_patricia_storage::db_object::{DBObject, Deserializable, HasStaticPrefix};
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;

use crate::generate_trie_config;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::external_test_utils::get_random_u256;
use crate::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use crate::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, NodeData, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::{Leaf, SkeletonLeaf};
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, SubTreeHeight};
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::{
    HashFunction,
    TreeHashFunction,
};
use crate::patricia_merkle_tree::updated_skeleton_tree::node::UpdatedSkeletonNode;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

#[derive(Debug, PartialEq, Clone, Copy, Default, Eq)]
pub struct MockLeaf(pub(crate) Felt);

impl HasStaticPrefix for MockLeaf {
    fn get_static_prefix() -> DbKeyPrefix {
        DbKeyPrefix::new(vec![0])
    }
}

impl DBObject for MockLeaf {
    fn serialize(&self) -> DbValue {
        DbValue(self.0.to_bytes_be().to_vec())
    }
}

impl Deserializable for MockLeaf {
    fn deserialize(value: &DbValue) -> Result<Self, DeserializationError> {
        Ok(Self(Felt::from_bytes_be_slice(&value.0)))
    }
}

impl Leaf for MockLeaf {
    type Input = Felt;
    type Output = String;

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    // Create a leaf with value equal to input. If input is `Felt::MAX`, returns an error.
    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        if input == Felt::MAX {
            return Err(LeafError::LeafComputationError("Leaf computation error".to_string()));
        }
        Ok((Self(input), input.to_hex_string()))
    }
}

pub(crate) struct TestTreeHashFunction;

impl TreeHashFunction<MockLeaf> for TestTreeHashFunction {
    fn compute_leaf_hash(leaf_data: &MockLeaf) -> HashOutput {
        HashOutput(leaf_data.0)
    }

    fn compute_node_hash(node_data: &NodeData<MockLeaf>) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<MockHashFunction>(node_data)
    }
}

generate_trie_config!(OriginalSkeletonMockTrieConfig, MockLeaf);

pub(crate) type MockTrie = FilledTreeImpl<MockLeaf>;

struct MockHashFunction;

impl HashFunction for MockHashFunction {
    fn hash(left: &Felt, right: &Felt) -> HashOutput {
        HashOutput(*left + *right)
    }
}

impl From<u8> for SkeletonLeaf {
    fn from(value: u8) -> Self {
        Self::from(Felt::from(value))
    }
}

impl From<&str> for PathToBottom {
    fn from(value: &str) -> Self {
        Self::new(
            U256::from_str_radix(value, 2).expect("Invalid binary string").into(),
            EdgePathLength::new(
                (value.len() - if value.starts_with('+') { 1 } else { 0 })
                    .try_into()
                    .expect("String is too large"),
            )
            .expect("Invalid length"),
        )
        .expect("Illegal PathToBottom")
    }
}

#[fixture]
pub(crate) fn random() -> ThreadRng {
    rand::thread_rng()
}

pub(crate) fn small_tree_index_to_full(index: U256, height: SubTreeHeight) -> NodeIndex {
    NodeIndex::from_subtree_index(NodeIndex::new(index), height)
}

#[rstest]
#[should_panic]
#[case(U256::ZERO, U256::ZERO)]
#[should_panic]
#[case(U256::ONE, U256::ZERO)]
#[case(U256::ZERO, U256::ONE)]
#[case(U256::ONE, U256::ONE << 128)]
#[case(U256::ONE<<62, U256::ONE << 128)]
#[case(U256::ONE<<128, (U256::ONE << 128)+U256::ONE)]
fn test_get_random_u256(mut random: ThreadRng, #[case] low: U256, #[case] high: U256) {
    let r = get_random_u256(&mut random, low, high);
    assert!(low <= r && r < high);
}

/// Returns an UpdatedSkeleton instance initialized with the UpdatedSkeletonNodes immediately
/// derived from the leaf_modifications (as done in UpdatedSkeletonTreeImpl::finalize_bottom_layer).
pub(crate) fn get_initial_updated_skeleton(
    original_skeleton: &[(NodeIndex, OriginalSkeletonNode)],
    leaf_modifications: &[(NodeIndex, u8)],
) -> UpdatedSkeletonTreeImpl {
    UpdatedSkeletonTreeImpl {
        skeleton_tree: leaf_modifications
            .iter()
            .filter(|(_, leaf_val)| *leaf_val != 0)
            .map(|(index, _)| (*index, UpdatedSkeletonNode::Leaf))
            .chain(original_skeleton.iter().filter_map(|(index, node)| match node {
                OriginalSkeletonNode::UnmodifiedSubTree(hash) => {
                    Some((*index, UpdatedSkeletonNode::UnmodifiedSubTree(*hash)))
                }
                OriginalSkeletonNode::Binary | OriginalSkeletonNode::Edge(_) => None,
            }))
            .collect(),
    }
}

pub(crate) fn as_fully_indexed(
    subtree_height: u8,
    indices: impl Iterator<Item = U256>,
) -> Vec<NodeIndex> {
    indices
        .map(|index| small_tree_index_to_full(index, SubTreeHeight::new(subtree_height)))
        .collect()
}
