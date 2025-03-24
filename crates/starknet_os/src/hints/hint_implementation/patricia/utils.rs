use std::collections::{HashMap, HashSet};
use std::ops::Sub;

use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{BinaryData, EdgeData};

use crate::hints::hint_implementation::patricia::error::PatriciaError;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

#[derive(Clone)]
pub enum Preimage {
    Binary(BinaryData),
    Edge(EdgeData),
}

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
            _ => Err(PatriciaError::ExpectedBinary),
        }
    }
}

pub type PreimageMap = HashMap<HashOutput, Preimage>;

#[derive(Clone, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}

pub type TreeIndex = BigUint;

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTreeInner {
    Tuple(Box<UpdateTree>, Box<UpdateTree>),
    Leaf(HashOutput),
}

pub type UpdateTree = Option<UpdateTreeInner>;
type Layer = HashMap<TreeIndex, UpdateTreeInner>;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Height(u8);

impl Sub<u8> for Height {
    type Output = Self;

    fn sub(self, rhs: u8) -> Self::Output {
        Self(self.0 - rhs)
    }
}

/// Constructs layers of a tree from leaf updates. This is not a full binary tree. It is just the
/// subtree induced by the modification leaves. Returns a tree of updates. A tree is built from
/// layers. Each layer represents the nodes in a specific height. The 0 layer is the root, and the
/// last layer is the leaves.
/// Each layer is a map from index in current merkle layer [0, 2**layer_height) to either:
/// * a leaf (new value) - if it's the last layer and the leaf is modified.
/// * a pair of indices, or a pair of None and an index, or a pair of an index and None - if it's an
///   internal node that has modified leaves/leaf in its subtree.
pub(crate) fn build_update_tree(
    height: Height,
    modifications: Vec<(TreeIndex, HashOutput)>,
) -> UpdateTree {
    // Bottom layer. This will prefer the last modification to an index.
    if modifications.is_empty() {
        return None;
    }

    // A layer is a map from index in current merkle layer [0, 2**layer_height) to a tree.
    // A tree is either None, a leaf, or a pair of trees.
    let mut layer: Layer = modifications
        .into_iter()
        .map(|(index, value)| (index, UpdateTreeInner::Leaf(value)))
        .collect();

    for _ in 0..height.0 {
        let parents: HashSet<TreeIndex> = layer.keys().map(|key| key / 2u64).collect();
        let mut new_layer: Layer = Layer::new();

        for index in parents.into_iter() {
            let left_update = layer.get(&(&index * 2u64)).cloned();
            let right_update = layer.get(&(&index * 2u64 + 1u64)).cloned();

            new_layer.insert(
                index,
                UpdateTreeInner::Tuple(Box::new(left_update), Box::new(right_update)),
            );
        }

        layer = new_layer;
    }

    // We reached layer_height=0, the top layer with only the root (with index 0).
    debug_assert!(layer.len() == 1);

    layer.remove(&0u64.into())
}
