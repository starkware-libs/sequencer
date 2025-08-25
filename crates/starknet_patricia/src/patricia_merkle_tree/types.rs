use ethnum::U256;
use starknet_types_core::felt::Felt;

use crate::felt::u256_from_felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::TypesError;
use crate::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use crate::patricia_merkle_tree::node_data::inner_node::{EdgePathLength, PathToBottom};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::original_skeleton_tree::utils::split_leaves;

#[cfg(test)]
#[path = "types_test.rs"]
pub mod types_test;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, derive_more::Sub, derive_more::Display)]
pub struct SubTreeHeight(pub u8);

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

impl From<SubTreeHeight> for Felt {
    fn from(value: SubTreeHeight) -> Self {
        value.0.into()
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
    pub const MAX: Self =
        Self(U256::from_words(u128::MAX >> (U256::BITS - Self::BITS as u32), u128::MAX));

    pub fn new(index: U256) -> Self {
        assert!(index <= Self::MAX.0, "Index {index} is too large.");
        Self(index)
    }

    pub fn is_leaf(&self) -> bool {
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
        [left_child, left_child + 1]
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

        PathToBottom::new(delta.0.into(), EdgePathLength::new(distance).expect("Illegal length"))
            .expect("Illegal PathToBottom")
    }

    pub fn from_leaf_felt(felt: &Felt) -> Self {
        Self::FIRST_LEAF + Self::from_felt_value(felt)
    }

    pub(crate) fn from_felt_value(felt: &Felt) -> Self {
        Self(u256_from_felt(felt))
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

impl std::ops::Add<u128> for NodeIndex {
    type Output = Self;

    fn add(self, rhs: u128) -> Self {
        self + Self::from(rhs)
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
        Self::new(U256::from(value))
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SortedLeafIndices<'a>(&'a [NodeIndex]);

impl<'a> SortedLeafIndices<'a> {
    /// Creates a new instance by sorting the given indices.
    // TODO(Nimrod, 1/8/2024): Remove duplicates from the given indices.
    pub fn new(indices: &'a mut [NodeIndex]) -> Self {
        indices.sort();
        Self(indices)
    }

    /// Returns a subslice of the indices stored at self, at the range [leftmost_idx,
    /// rightmost_idx).
    pub fn subslice(&self, leftmost_idx: usize, rightmost_idx: usize) -> Self {
        Self(&self.0[leftmost_idx..rightmost_idx])
    }

    /// Divides the slice held by self into two instances. One holds the range [0, idx), the
    /// other holds the range [idx, len(self)).
    pub(crate) fn divide_at_index(&self, idx: usize) -> [Self; 2] {
        [Self(&self.0[..idx]), Self(&self.0[idx..])]
    }

    pub(crate) fn get_indices(&self) -> &'a [NodeIndex] {
        self.0
    }

    pub(crate) fn contains(&self, value: &NodeIndex) -> bool {
        self.0.contains(value)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn last(&self) -> Option<&NodeIndex> {
        self.0.last()
    }

    pub(crate) fn first(&self) -> Option<&NodeIndex> {
        self.0.first()
    }

    /// Returns the leftmost position where `leftmost_value` can be inserted to the slice and
    /// maintain sorted order. Assumes that the elements in the slice are unique.
    pub(crate) fn bisect_left(&self, leftmost_value: &NodeIndex) -> usize {
        match self.0.binary_search(leftmost_value) {
            Ok(pos) | Err(pos) => pos,
        }
    }

    /// Returns the rightmost position where `rightmost_value` can be inserted to the slice and
    /// maintain sorted order. Assumes that the elements in the slice are unique.
    pub(crate) fn bisect_right(&self, rightmost_value: &NodeIndex) -> usize {
        match self.0.binary_search(rightmost_value) {
            Err(pos) => pos,
            Ok(pos) => pos + 1,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct SubTree<'a> {
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
    pub root_index: NodeIndex,
    pub root_hash: HashOutput,
}

impl<'a> SubTree<'a> {
    pub(crate) fn get_height(&self) -> SubTreeHeight {
        SubTreeHeight::new(SubTreeHeight::ACTUAL_HEIGHT.0 - (self.root_index.bit_length() - 1))
    }

    pub(crate) fn split_leaves(&self) -> [SortedLeafIndices<'a>; 2] {
        split_leaves(&self.root_index, &self.sorted_leaf_indices)
    }

    pub(crate) fn is_unmodified(&self) -> bool {
        self.sorted_leaf_indices.is_empty()
    }

    pub(crate) fn get_root_prefix<L: Leaf>(&self) -> PatriciaPrefix {
        if self.is_leaf() {
            PatriciaPrefix::Leaf(L::get_static_prefix())
        } else {
            PatriciaPrefix::InnerNode
        }
    }

    /// Returns the bottom subtree which is referred from `self` by the given path. When creating
    /// the bottom subtree some indices that were modified under `self` are not modified under the
    /// bottom subtree (leaves that were previously empty). These indices are returned as well.
    pub(crate) fn get_bottom_subtree(
        &self,
        path_to_bottom: &PathToBottom,
        bottom_hash: HashOutput,
    ) -> (Self, Vec<&NodeIndex>) {
        let bottom_index = path_to_bottom.bottom_index(self.root_index);
        let bottom_height = self.get_height() - SubTreeHeight::new(path_to_bottom.length.into());
        let leftmost_in_subtree = bottom_index << bottom_height.into();
        let rightmost_in_subtree =
            leftmost_in_subtree - NodeIndex::ROOT + (NodeIndex::ROOT << bottom_height.into());
        let leftmost_index = self.sorted_leaf_indices.bisect_left(&leftmost_in_subtree);
        let rightmost_index = self.sorted_leaf_indices.bisect_right(&rightmost_in_subtree);
        let bottom_leaves = self.sorted_leaf_indices.subslice(leftmost_index, rightmost_index);
        let previously_empty_leaf_indices = self.sorted_leaf_indices.get_indices()
            [..leftmost_index]
            .iter()
            .chain(self.sorted_leaf_indices.get_indices()[rightmost_index..].iter())
            .collect();

        (
            Self {
                sorted_leaf_indices: bottom_leaves,
                root_index: bottom_index,
                root_hash: bottom_hash,
            },
            previously_empty_leaf_indices,
        )
    }

    pub(crate) fn get_children_subtrees(
        &self,
        left_hash: HashOutput,
        right_hash: HashOutput,
    ) -> (Self, Self) {
        let [left_leaves, right_leaves] = self.split_leaves();
        let left_root_index = self.root_index * 2.into();
        (
            SubTree {
                sorted_leaf_indices: left_leaves,
                root_index: left_root_index,
                root_hash: left_hash,
            },
            SubTree {
                sorted_leaf_indices: right_leaves,
                root_index: left_root_index + NodeIndex::ROOT,
                root_hash: right_hash,
            },
        )
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.root_index.is_leaf()
    }
}
