use crate::block_committer::input::{ContractAddress, StarknetStorageKey};
use crate::felt::Felt;
use crate::patricia_merkle_tree::filled_tree::node::ClassHash;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

use ethnum::U256;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Sub)]
pub struct TreeHeight(pub u8);

impl TreeHeight {
    pub const MAX_HEIGHT: u8 = 251;
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::BitAnd, derive_more::Sub, PartialOrd, Ord,
)]
pub(crate) struct NodeIndex(U256);

// Wraps a U256. Maximal possible value is the largest index in a tree of height 251 (2 ^ 252 - 1).
impl NodeIndex {
    pub const BITS: u8 = TreeHeight::MAX_HEIGHT + 1;

    /// [NodeIndex] constant that represents the root index.
    pub const ROOT: Self = Self(U256::ONE);

    /// [NodeIndex] constant that represents the largest index in a tree.
    #[allow(clippy::as_conversions)]
    pub const MAX_INDEX: Self = Self(U256::from_words(
        u128::MAX >> (U256::BITS - Self::BITS as u32),
        u128::MAX,
    ));

    pub(crate) fn new(index: U256) -> Self {
        if index > Self::MAX_INDEX.0 {
            panic!("Index {index} is too large.");
        }
        Self(index)
    }

    // TODO(Amos, 1/5/2024): Move to EdgePath.
    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: &PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        (index << length.0) + Self::from_felt_value(&path.0)
    }

    /// Returns the number of leading zeroes when represented with Self::BITS bits.
    pub(crate) fn leading_zeros(&self) -> u8 {
        (self.0.leading_zeros() - (U256::BITS - u32::from(Self::BITS)))
            .try_into()
            .expect("Leading zeroes are unexpectedly larger than a u8.")
    }

    pub(crate) fn bit_length(&self) -> u8 {
        Self::BITS - self.leading_zeros()
    }

    pub(crate) fn from_starknet_storage_key(
        key: &StarknetStorageKey,
        tree_height: &TreeHeight,
    ) -> Self {
        Self::from_leaf_felt(&key.0, tree_height)
    }

    pub(crate) fn from_contract_address(
        address: &ContractAddress,
        tree_height: &TreeHeight,
    ) -> Self {
        Self::from_leaf_felt(&address.0, tree_height)
    }

    pub(crate) fn from_class_hash(class_hash: &ClassHash, tree_height: &TreeHeight) -> Self {
        Self::from_leaf_felt(&class_hash.0, tree_height)
    }

    fn from_leaf_felt(felt: &Felt, tree_height: &TreeHeight) -> Self {
        Self(U256::from(1_u8) << tree_height.0) + Self::from_felt_value(felt)
    }

    fn from_felt_value(felt: &Felt) -> Self {
        NodeIndex(U256::from_be_bytes(felt.to_bytes_be()))
    }
}

impl std::ops::Add for NodeIndex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.0 + rhs.0)
    }
}

impl std::ops::Mul for NodeIndex {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self::new(self.0 * rhs.0)
    }
}

impl std::ops::Shl<u8> for NodeIndex {
    type Output = Self;

    /// Returns the index of the left descendant (child for rhs=1) of the node.
    fn shl(self, rhs: u8) -> Self::Output {
        Self::new(self.0 << rhs)
    }
}

impl std::ops::Shr<u8> for NodeIndex {
    type Output = Self;

    /// Returns the index of the ancestor (parent for rhs=1) of the node.
    fn shr(self, rhs: u8) -> Self::Output {
        Self::new(self.0 >> rhs)
    }
}

impl From<u128> for NodeIndex {
    fn from(value: u128) -> Self {
        Self(U256::from(value))
    }
}

impl From<NodeIndex> for U256 {
    fn from(value: NodeIndex) -> Self {
        value.0
    }
}
