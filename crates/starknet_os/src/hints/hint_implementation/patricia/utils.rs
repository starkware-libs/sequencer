use std::collections::{HashMap, HashSet};

use ethnum::U256;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;

use crate::hints::hint_implementation::patricia::error::PatriciaError;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

#[derive(Clone, Debug)]
pub enum Preimage {
    Binary(BinaryData),
    Edge(EdgeData),
}

pub type PreimageMap = HashMap<HashOutput, Preimage>;

impl Preimage {
    pub fn length(&self) -> u8 {
        match self {
            Preimage::Binary(_) => 2,
            Preimage::Edge(_) => 3,
        }
    }

    fn get_binary(&self) -> Result<&BinaryData, PatriciaError> {
        match self {
            Preimage::Binary(binary) => Ok(binary),
            _ => Err(PatriciaError::ExpectedBinary(self.clone())),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::Display)]
pub struct LayerIndex(U256);

#[allow(clippy::as_conversions)]
impl LayerIndex {
    pub const MAX: Self = Self(U256::from_words(
        u128::MAX >> (U256::BITS - SubTreeHeight::ACTUAL_HEIGHT.0 as u32),
        u128::MAX,
    ));
    pub const ROOT: Self = Self(U256::ZERO);

    pub fn new(index: U256) -> Result<Self, PatriciaError> {
        if index > Self::MAX.0 {
            return Err(PatriciaError::MaxLayerIndexExceeded(index));
        }
        Ok(Self(index))
    }

    pub fn get_children_indices(&self) -> Result<(Self, Self), PatriciaError> {
        let left_child = Self::new(self.0 << 1)?;
        let right_child = Self::new((self.0 << 1) + U256::ONE)?;
        Ok((left_child, right_child))
    }

    pub fn get_parent_index(&self) -> Result<Self, PatriciaError> {
        Self::new(self.0 >> 1)
    }
}

impl From<u128> for LayerIndex {
    fn from(value: u128) -> Self {
        // It's safe to unwrap because u128 is always less than MAX.
        Self::new(U256::from(value)).unwrap()
    }
}

/// Cases: both, if both children are to be updated, and left or right, if only one child is to be
/// updated.
#[derive(Clone, Debug, PartialEq)]
pub enum InnerNode {
    Left(Box<UpdateTree>),
    Right(Box<UpdateTree>),
    Both(Box<UpdateTree>, Box<UpdateTree>),
}

impl InnerNode {
    fn new(left: UpdateTree, right: UpdateTree) -> Result<Self, PatriciaError> {
        match (left, right) {
            (Some(left), Some(right)) => {
                Ok(Self::Both(Box::new(Some(left)), Box::new(Some(right))))
            }
            (Some(left), None) => Ok(Self::Left(Box::new(Some(left)))),
            (None, Some(right)) => Ok(Self::Right(Box::new(Some(right)))),
            (None, None) => Err(PatriciaError::InvalidInnerNode),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTreeInner {
    InnerNode(InnerNode),
    Leaf(HashOutput),
}

pub type UpdateTree = Option<UpdateTreeInner>;
type TreeLayer = HashMap<LayerIndex, UpdateTreeInner>;

/// Constructs layers of a tree from leaf updates. This is not a full binary tree. It is just the
/// subtree induced by the modification leaves. Returns a tree of updates. A tree is built from
/// layers. Each layer represents the nodes in a specific height. The 0 layer is the root, and the
/// last layer is the leaves.
/// Each layer is a map from index in current merkle layer [0, 2**layer_height) to either:
/// * a leaf (new value) - if it's the last layer and the leaf is modified.
/// * a pair of indices, or a pair of None and an index, or a pair of an index and None - if it's an
///   internal node that has modified leaves/leaf in its subtree.
pub(crate) fn build_update_tree(
    height: SubTreeHeight,
    modifications: Vec<(LayerIndex, HashOutput)>,
) -> Result<UpdateTree, PatriciaError> {
    // Bottom layer. This will prefer the last modification to an index.
    if modifications.is_empty() {
        return Ok(None);
    }

    // A layer is a map from index in current merkle layer [0, 2**layer_height) to a tree.
    // A tree is either None, a leaf, or a pair of trees.
    let mut layer: TreeLayer = modifications
        .into_iter()
        .map(|(index, value)| (index, UpdateTreeInner::Leaf(value)))
        .collect();

    for h in 0..height.into() {
        let parents: HashSet<LayerIndex> =
            layer.keys().map(|key| key.get_parent_index()).collect::<Result<HashSet<_>, _>>()?;
        let mut new_layer: TreeLayer = TreeLayer::new();

        for index in parents.into_iter() {
            let (left, right) = index.get_children_indices()?;
            let left_update = layer.get(&left).cloned();
            let right_update = layer.get(&right).cloned();

            let inner_node = InnerNode::new(left_update, right_update).map_err(|_| {
                PatriciaError::BothChildrenAreNone { index, height: SubTreeHeight(h) }
            })?;

            new_layer.insert(index, UpdateTreeInner::InnerNode(inner_node));
        }

        layer = new_layer;
    }

    // We reached layer_height=0, the top layer with only the root (with index 0).
    debug_assert!(layer.len() == 1);

    // Pop out and return the root node.
    Ok(layer.remove(&LayerIndex::ROOT))
}
