use std::collections::HashMap;

use ethnum::U256;
use num_bigint::{BigUint, RandBigInt};
use rand::Rng;
use starknet_api::hash::HashOutput;
use starknet_patricia_storage::db_object::{
    DBObject,
    EmptyDeserializationContext,
    EmptyKeyContext,
    HasStaticPrefix,
};
use starknet_patricia_storage::errors::{DeserializationError, SerializationResult};
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash;

use super::filled_tree::node_serde::PatriciaPrefix;
use super::node_data::inner_node::{EdgePathLength, PathToBottom};
use super::node_data::leaf::Leaf;
use super::original_skeleton_tree::node::OriginalSkeletonNode;
use super::types::{NodeIndex, SubTreeHeight};
use crate::db_layout::TrieType;
use crate::felt::u256_from_felt;
use crate::patricia_merkle_tree::errors::TypesError;
use crate::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};

pub(crate) const TEST_PREFIX: &[u8] = &[0];

#[derive(Debug, PartialEq, Clone, Copy, Default, Eq)]
pub struct MockLeaf(pub Felt);

#[derive(Debug, PartialEq, Clone, Copy, Default, Eq, derive_more::From)]
pub struct MockIndexLayoutLeaf(pub MockLeaf);

/// A mock leaf with KeyContext = TrieType
impl HasStaticPrefix for MockIndexLayoutLeaf {
    type KeyContext = TrieType;
    fn get_static_prefix(_key_context: &Self::KeyContext) -> DbKeyPrefix {
        DbKeyPrefix::new(TEST_PREFIX.into())
    }
}

impl DBObject for MockIndexLayoutLeaf {
    type DeserializeContext = EmptyDeserializationContext;
    fn serialize(&self) -> SerializationResult<DbValue> {
        self.0.serialize()
    }
    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
        Ok(Self(MockLeaf::deserialize(value, _deserialize_context)?))
    }
}

impl Leaf for MockIndexLayoutLeaf {
    type Input = Felt;
    type Output = String;

    fn is_empty(&self) -> bool {
        self.0.0 == Felt::ZERO
    }

    // Create a leaf with value equal to input. If input is `Felt::MAX`, returns an error.
    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        MockLeaf::create(input).await.map(|(leaf, output)| (Self(leaf), output))
    }
}

impl HasStaticPrefix for MockLeaf {
    type KeyContext = EmptyKeyContext;
    fn get_static_prefix(_key_context: &Self::KeyContext) -> DbKeyPrefix {
        DbKeyPrefix::new(TEST_PREFIX.into())
    }
}

impl DBObject for MockLeaf {
    type DeserializeContext = EmptyDeserializationContext;

    fn serialize(&self) -> SerializationResult<DbValue> {
        Ok(DbValue(self.0.to_bytes_be().to_vec()))
    }

    fn deserialize(
        value: &DbValue,
        _deserialize_context: &Self::DeserializeContext,
    ) -> Result<Self, DeserializationError> {
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

pub fn u256_try_into_felt(value: &U256) -> Result<Felt, TypesError<U256>> {
    if *value > u256_from_felt(&Felt::MAX) {
        return Err(TypesError::ConversionError {
            from: *value,
            to: "Felt",
            reason: "value is bigger than felt::max",
        });
    }
    Ok(Felt::from_bytes_be(&value.to_be_bytes()))
}

/// Generates a random U256 number between low (inclusive) and high (exclusive).
/// Panics if low >= high
pub fn get_random_u256<R: Rng>(rng: &mut R, low: U256, high: U256) -> U256 {
    assert!(low < high, "low must be less than or equal to high. actual: {low} > {high}");

    let delta = BigUint::from_bytes_be(&(high - low).to_be_bytes());
    let rand = rng.gen_biguint_below(&(delta)).to_bytes_be();
    let mut padded_rand = [0u8; 32];
    padded_rand[32 - rand.len()..].copy_from_slice(&rand);
    low + U256::from_be_bytes(padded_rand)
}

pub struct AdditionHash;

impl StarkHash for AdditionHash {
    fn hash(felt_0: &Felt, felt_1: &Felt) -> Felt {
        *felt_0 + *felt_1
    }

    fn hash_array(felts: &[Felt]) -> Felt {
        felts.iter().fold(Felt::ZERO, |acc, felt| acc + *felt)
    }

    fn hash_single(felt: &Felt) -> Felt {
        *felt
    }
}

fn hash_edge<SH: StarkHash>(hash: &Felt, path: &Felt, length: &Felt) -> Felt {
    SH::hash(hash, path) + *length
}

pub fn create_32_bytes_entry(simple_val: u128) -> [u8; 32] {
    U256::from(simple_val).to_be_bytes()
}

fn create_inner_node_patricia_key(val: Felt) -> DbKey {
    create_db_key(PatriciaPrefix::InnerNode.into(), &val.to_bytes_be())
}

pub fn create_leaf_patricia_key<L: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>>(
    val: u128,
) -> DbKey {
    create_db_key(L::get_static_prefix(&EmptyKeyContext), &U256::from(val).to_be_bytes())
}

fn create_binary_val(left: Felt, right: Felt) -> DbValue {
    DbValue((left.to_bytes_be().into_iter().chain(right.to_bytes_be())).collect())
}

fn create_edge_val(hash: Felt, path: u128, length: u8) -> DbValue {
    DbValue(
        hash.to_bytes_be().into_iter().chain(create_32_bytes_entry(path)).chain([length]).collect(),
    )
}

pub fn create_binary_entry<SH: StarkHash>(left: Felt, right: Felt) -> (DbKey, DbValue) {
    (create_inner_node_patricia_key(SH::hash(&left, &right)), create_binary_val(left, right))
}

pub fn create_binary_entry_from_u128<SH: StarkHash>(left: u128, right: u128) -> (DbKey, DbValue) {
    create_binary_entry::<SH>(Felt::from(left), Felt::from(right))
}

pub fn create_edge_entry<SH: StarkHash>(hash: Felt, path: u128, length: u8) -> (DbKey, DbValue) {
    (
        create_inner_node_patricia_key(hash_edge::<SH>(
            &hash,
            &Felt::from(path),
            &Felt::from(length),
        )),
        create_edge_val(hash, path, length),
    )
}

pub fn create_edge_entry_from_u128<SH: StarkHash>(
    hash: u128,
    path: u128,
    length: u8,
) -> (DbKey, DbValue) {
    create_edge_entry::<SH>(Felt::from(hash), path, length)
}

pub fn create_leaf_entry<L: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>>(
    hash: u128,
) -> (DbKey, DbValue) {
    (create_leaf_patricia_key::<L>(hash), DbValue(create_32_bytes_entry(hash).to_vec()))
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

pub fn create_root_edge_entry(old_root: u128, subtree_height: SubTreeHeight) -> (DbKey, DbValue) {
    // Assumes path is 0.
    let length = SubTreeHeight::ACTUAL_HEIGHT.0 - subtree_height.0;
    let new_root = old_root + u128::from(length);
    let key = create_db_key(PatriciaPrefix::InnerNode.into(), &Felt::from(new_root).to_bytes_be());
    let value = DbValue(
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

pub fn small_tree_index_to_full(index: U256, height: SubTreeHeight) -> NodeIndex {
    NodeIndex::from_subtree_index(NodeIndex::new(index), height)
}

pub fn as_fully_indexed(subtree_height: u8, indices: impl Iterator<Item = U256>) -> Vec<NodeIndex> {
    indices
        .map(|index| small_tree_index_to_full(index, SubTreeHeight::new(subtree_height)))
        .collect()
}
