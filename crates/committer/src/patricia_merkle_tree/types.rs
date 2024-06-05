use crate::block_committer::input::{ContractAddress, StarknetStorageKey};
use crate::felt::Felt;
use crate::patricia_merkle_tree::errors::TypesError;
use crate::patricia_merkle_tree::filled_tree::node::ClassHash;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};

use ethnum::U256;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Sub, derive_more::Display)]
pub struct SubTreeHeight(pub(crate) u8);

impl SubTreeHeight {
    pub const ACTUAL_HEIGHT: SubTreeHeight = SubTreeHeight(251);

    pub fn new(height: u8) -> Self {
        if height > Self::ACTUAL_HEIGHT.0 {
            panic!("Height {height} is too large.");
        }
        Self(height)
    }
}

impl From<SubTreeHeight> for u8 {
    fn from(value: SubTreeHeight) -> Self {
        value.0
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::BitAnd, derive_more::Sub, PartialOrd, Ord,
)]
pub struct NodeIndex(pub U256);

// Wraps a U256. Maximal possible value is the largest index in a tree of height 251 (2 ^ 252 - 1).
impl NodeIndex {
    pub const BITS: u8 = SubTreeHeight::ACTUAL_HEIGHT.0 + 1;

    /// [NodeIndex] constant that represents the root index.
    pub const ROOT: Self = Self(U256::ONE);

    /// [NodeIndex] constant that represents the first leaf index.
    // TODO(Tzahi, 15/6/2024): Support height < 128 bits.
    pub const FIRST_LEAF: Self = Self(U256::from_words(1_u128 << (Self::BITS - 1 - 128), 0));

    #[allow(clippy::as_conversions)]
    /// [NodeIndex] constant that represents the largest index in a tree.
    // TODO(Tzahi, 15/6/2024): Support height < 128 bits.
    pub const MAX: Self = Self(U256::from_words(
        u128::MAX >> (U256::BITS - Self::BITS as u32),
        u128::MAX,
    ));

    pub fn new(index: U256) -> Self {
        assert!(index <= Self::MAX.0, "Index {index} is too large.");
        Self(index)
    }

    pub(crate) fn is_leaf(&self) -> bool {
        Self::FIRST_LEAF <= *self && *self <= Self::MAX
    }

    // TODO(Amos, 1/5/2024): Move to EdgePath.
    pub(crate) fn compute_bottom_index(
        index: NodeIndex,
        path_to_bottom: &PathToBottom,
    ) -> NodeIndex {
        let PathToBottom { path, length, .. } = path_to_bottom;
        (index << u8::from(*length)) + Self::new(path.into())
    }

    pub(crate) fn get_children_indices(&self) -> [Self; 2] {
        let left_child = *self << 1;
        [left_child, left_child + 1.into()]
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

    /// Get the LCA (Lowest Common Ancestor) of the two nodes.
    pub(crate) fn get_lca(&self, other: &NodeIndex) -> NodeIndex {
        if self == other {
            return *self;
        }

        let bit_length = self.bit_length();
        let other_bit_length = other.bit_length();
        // Bring self and other to a common (low) height.
        let (adapted_self, adapted_other) = if self < other {
            (*self, *other >> (other_bit_length - bit_length))
        } else {
            (*self >> (bit_length - other_bit_length), *other)
        };

        let xor = adapted_self.0 ^ adapted_other.0;
        // The length of the remainder after removing the common prefix of the two nodes.
        let post_common_prefix_len = NodeIndex::new(xor).bit_length();

        let lca = adapted_self.0 >> post_common_prefix_len;
        NodeIndex::new(lca)
    }

    /// Returns the path from the node to its given descendant (0 length if node == descendant).
    /// Panics if the supposed descendant is not really a descendant.
    pub(crate) fn get_path_to_descendant(&self, descendant: Self) -> PathToBottom {
        let descendant_bit_length = descendant.bit_length();
        let bit_length = self.bit_length();
        if bit_length > descendant_bit_length {
            panic!("The descendant is not a really descendant of the node.");
        };

        let distance = descendant_bit_length - bit_length;
        let delta = descendant - (*self << distance);
        if descendant >> distance != *self {
            panic!("The descendant is not a really descendant of the node.");
        };

        PathToBottom::new(
            delta.0.into(),
            EdgePathLength::new(distance).expect("Illegal length"),
        )
        .expect("Illegal PathToBottom")
    }

    pub(crate) fn from_starknet_storage_key(key: &StarknetStorageKey) -> Self {
        Self::from_leaf_felt(&key.0)
    }

    pub(crate) fn from_contract_address(address: &ContractAddress) -> Self {
        Self::from_leaf_felt(&address.0)
    }

    pub(crate) fn from_class_hash(class_hash: &ClassHash) -> Self {
        Self::from_leaf_felt(&class_hash.0)
    }

    fn from_leaf_felt(felt: &Felt) -> Self {
        Self::FIRST_LEAF + Self::from_felt_value(felt)
    }

    fn from_felt_value(felt: &Felt) -> Self {
        Self(U256::from(felt))
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

impl TryFrom<NodeIndex> for Felt {
    type Error = TypesError<NodeIndex>;

    fn try_from(value: NodeIndex) -> Result<Self, Self::Error> {
        if value.0 > U256::from_be_bytes(Self::MAX.to_bytes_be()) {
            return Err(TypesError::ConversionError {
                from: value,
                to: "Felt",
                reason: "NodeIndex is too large",
            });
        }
        let bytes = value.0.to_be_bytes();
        Ok(Self::from_bytes_be_slice(&bytes))
    }
}
