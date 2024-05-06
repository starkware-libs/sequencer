use crate::block_committer::input::StarknetStorageKey;
use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

use ethnum::U256;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

#[allow(dead_code)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    derive_more::Add,
    derive_more::Mul,
    derive_more::Sub,
    PartialOrd,
    Ord,
)]
pub(crate) struct NodeIndex(pub U256);

#[allow(dead_code)]
// Wraps a U256. Maximal possible value is the largest index in a tree of height 251 (2 ^ 252 - 1).
impl NodeIndex {
    pub(crate) fn root_index() -> NodeIndex {
        NodeIndex(U256::ONE)
    }

    // TODO(Amos, 1/5/2024): Move to EdgePath.
    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: &PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length } = path_to_bottom;
        (index << length.0) + NodeIndex::from(path.0)
    }

    pub(crate) fn bit_length(&self) -> u8 {
        (U256::BITS - self.0.leading_zeros())
            .try_into()
            .expect("Failed to convert to u8.")
    }

    pub(crate) fn from_starknet_storage_key(
        key: &StarknetStorageKey,
        tree_height: &TreeHeight,
    ) -> Self {
        Self(U256::from(1_u8) << tree_height.0) + Self::from(key.0)
    }
}

impl std::ops::Shl<u8> for NodeIndex {
    type Output = Self;

    /// Returns the index of the left descendant (child for rhs=1) of the node.
    fn shl(self, rhs: u8) -> Self::Output {
        NodeIndex(self.0 << rhs)
    }
}

impl std::ops::Shr<u8> for NodeIndex {
    type Output = Self;

    /// Returns the index of the ancestor (parent for rhs=1) of the node.
    fn shr(self, rhs: u8) -> Self::Output {
        NodeIndex(self.0 >> rhs)
    }
}

impl From<u128> for NodeIndex {
    fn from(value: u128) -> Self {
        Self(U256::from(value))
    }
}

impl From<Felt> for NodeIndex {
    fn from(value: Felt) -> Self {
        Self(U256::from_be_bytes(value.to_bytes_be()))
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Sub)]
pub struct TreeHeight(pub u8);
