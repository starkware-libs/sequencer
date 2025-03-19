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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerIndex(U256);

impl LayerIndex {
    pub fn new(index: U256) -> Self {
        Self(index)
    }

    pub fn get_children_indices(&self) -> (Self, Self) {
        let left_child = Self::new(self.0 << 1);
        let right_child = Self::new((self.0 << 1) + U256::ONE);
        (left_child, right_child)
    }

    pub fn get_parent_index(&self) -> Self {
        Self::new(self.0 >> 1)
    }
}

impl From<u128> for LayerIndex {
    fn from(value: u128) -> Self {
        Self::new(U256::from(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTreeInner {
    InnerNode(Box<UpdateTree>, Box<UpdateTree>),
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
) -> UpdateTree {
    // Bottom layer. This will prefer the last modification to an index.
    if modifications.is_empty() {
        return None;
    }

    // A layer is a map from index in current merkle layer [0, 2**layer_height) to a tree.
    // A tree is either None, a leaf, or a pair of trees.
    let mut layer: TreeLayer = modifications
        .into_iter()
        .map(|(index, value)| (index, UpdateTreeInner::Leaf(value)))
        .collect();

    for _ in 0..height.into() {
        let parents: HashSet<LayerIndex> = layer.keys().map(|key| key.get_parent_index()).collect();
        let mut new_layer: TreeLayer = TreeLayer::new();

        for index in parents.into_iter() {
            let (left, right) = index.get_children_indices();
            let left_update = layer.get(&left).cloned();
            let right_update = layer.get(&right).cloned();

            new_layer.insert(
                index,
                UpdateTreeInner::InnerNode(Box::new(left_update), Box::new(right_update)),
            );
        }

        layer = new_layer;
    }

    // We reached layer_height=0, the top layer with only the root (with index 0).
    debug_assert!(layer.len() == 1);

    layer.remove(&0u128.into())
}
