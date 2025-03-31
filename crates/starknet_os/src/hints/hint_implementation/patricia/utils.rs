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
            Preimage::Edge(_) => Err(PatriciaError::ExpectedBinary(self.clone())),
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
    // SubTreeHeight::ACTUAL_HEIGHT is expected to be > 128 (otherwise this will fail to compile).
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
        let right_child = Self::new(left_child.0 + U256::ONE)?;
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

/// Variants correspond to the required updates: `both` if both children are to be updated, and
/// `left` or `right` if only a single child is to be updated.

#[derive(Clone, Debug, PartialEq)]
pub enum InnerNode {
    Left(Box<UpdateTree>),
    Right(Box<UpdateTree>),
    Both(Box<UpdateTree>, Box<UpdateTree>),
}

// TODO(Rotem): Maybe we can avoid using None.
// Update when implementing the other functions.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTree {
    InnerNode(InnerNode),
    Leaf(HashOutput),
    // Represents a node where none of it's descendants has been modified.
    None,
}

type TreeLayer = HashMap<LayerIndex, UpdateTree>;

/// Constructs layers of a tree from leaf updates. This is not a full binary tree. It is just the
/// subtree induced by the modification leaves. Returns a tree of updates. A tree is built from
/// layers. Each layer represents the nodes in a specific height. The top layer is the root, and the
/// bottom layer holds the leaves.
/// Each layer is a map from an index in the current Merkle layer [0, 2**layer_height) to either:
/// * a leaf (new value) - if it's the bottom layer and the leaf is modified.
pub(crate) fn build_update_tree(
    height: SubTreeHeight,
    modifications: Vec<(LayerIndex, HashOutput)>,
) -> Result<UpdateTree, PatriciaError> {
    if modifications.is_empty() {
        return Ok(UpdateTree::None);
    }

    // A layer is a map from index in current merkle layer [0, 2**layer_height) to a tree.
    // A tree is either None, a leaf, or a pair of trees.
    let mut layer: TreeLayer =
        modifications.into_iter().map(|(index, value)| (index, UpdateTree::Leaf(value))).collect();

    for h in 0..height.into() {
        let parents: HashSet<LayerIndex> =
            layer.keys().map(|key| key.get_parent_index()).collect::<Result<HashSet<_>, _>>()?;
        let mut new_layer: TreeLayer = TreeLayer::new();

        for index in parents.into_iter() {
            let (left, right) = index.get_children_indices()?;
            let left_update = layer.remove(&left);
            let right_update = layer.remove(&right);

            let inner_node = match (left_update, right_update) {
                (Some(left), Some(right)) => InnerNode::Both(Box::new(left), Box::new(right)),
                (Some(left), None) => InnerNode::Left(Box::new(left)),
                (None, Some(right)) => InnerNode::Right(Box::new(right)),
                (None, None) => {
                    return Err(PatriciaError::BothChildrenAreNone {
                        index,
                        height: SubTreeHeight(h),
                    });
                }
            };

            new_layer.insert(index, UpdateTree::InnerNode(inner_node));
        }

        layer = new_layer;
    }

    // We reached layer_height=0, the top layer with only the root (with index 0).
    debug_assert!(layer.len() == 1);

    // Pop out and return the root node.
    Ok(layer
        .remove(&LayerIndex::ROOT)
        .expect("There should be a root node since modifications are not empty."))
}
