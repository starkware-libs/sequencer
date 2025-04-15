use std::collections::{HashMap, HashSet};

use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePathLength,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

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
    pub const BINARY_LENGTH: u8 = 2;
    pub const EDGE_LENGTH: u8 = 3;

    pub fn length(&self) -> u8 {
        match self {
            Preimage::Binary(_) => Self::BINARY_LENGTH,
            Preimage::Edge(_) => Self::EDGE_LENGTH,
        }
    }

    fn get_binary(&self) -> Result<&BinaryData, PatriciaError> {
        match self {
            Preimage::Binary(binary) => Ok(binary),
            Preimage::Edge(_) => Err(PatriciaError::ExpectedBinary(self.clone())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodeNodeCase {
    Left,
    Right,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, derive_more::Display)]
pub struct LayerIndex(BigUint);

#[allow(clippy::as_conversions)]
impl LayerIndex {
    pub const FIRST_LEAF: Self = Self(BigUint::ZERO);

    // SubTreeHeight::ACTUAL_HEIGHT is expected to be > 128 (otherwise this will fail to compile).
    // Note that this is not the same as
    // `starknet_patricia::patricia_merkle_tree::types::NodeIndex::MAX`, because this type
    // indexes a layer, not the entire tree.
    pub fn max() -> Self {
        Self((BigUint::from(1u128) << SubTreeHeight::ACTUAL_HEIGHT.0) - 1u128)
    }

    pub fn new(index: BigUint) -> Result<Self, PatriciaError> {
        if index > Self::max().0 {
            return Err(PatriciaError::MaxLayerIndexExceeded(index));
        }
        Ok(Self(index))
    }

    pub fn get_children_indices(&self) -> Result<(Self, Self), PatriciaError> {
        let left_child = Self::new(self.clone().0 << 1)?;
        let right_child = Self::new(left_child.clone().0 + BigUint::from(1u128))?;
        Ok((left_child, right_child))
    }

    pub fn get_parent_index(&self) -> Result<Self, PatriciaError> {
        Self::new(self.clone().0 >> 1)
    }
}

impl From<u128> for LayerIndex {
    fn from(value: u128) -> Self {
        // It's safe to unwrap because u128 is always less than MAX.
        Self::new(BigUint::from(value)).expect("u128::MAX is less than the max layer index.")
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

impl InnerNode {
    fn get_children(&self) -> (&UpdateTree, &UpdateTree) {
        match self {
            InnerNode::Left(left) => (left, &UpdateTree::None),
            InnerNode::Right(right) => (&UpdateTree::None, right),
            InnerNode::Both(left, right) => (left, right),
        }
    }
}

impl From<&InnerNode> for DecodeNodeCase {
    fn from(inner_node: &InnerNode) -> Self {
        match inner_node {
            InnerNode::Left(_) => DecodeNodeCase::Left,
            InnerNode::Right(_) => DecodeNodeCase::Right,
            InnerNode::Both(_, _) => DecodeNodeCase::Both,
        }
    }
}

// TODO(Rotem): Maybe we can avoid using None.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTree {
    InnerNode(InnerNode),
    Leaf(HashOutput),
    // Represents a node where none of it's descendants has been modified.
    None,
}

type TreeLayer = HashMap<LayerIndex, UpdateTree>;

/// Constructs layers of a tree from leaf updates. This is not a full state tree, it is just the
/// subtree induced by the modification leaves.
/// Returns a tree of updates. A tree is built from layers, where each layer represents the nodes in
/// a specific height. The top layer is the root, and the bottom layer holds the leaves.
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
                    unreachable!("Expected non-empty tree at index {index}, height {h}.")
                }
            };

            new_layer.insert(index, UpdateTree::InnerNode(inner_node));
        }
        layer = new_layer;
    }

    // We reached layer_height=0, the top layer with only the root (with index 0).
    debug_assert!(layer.len() == 1);

    // Pop out and return the root node, which is the first leaf in the top layer.
    Ok(layer
        .remove(&LayerIndex::FIRST_LEAF)
        .expect("There should be a root node since modifications are not empty."))
}

/// Deserializes the preimage mapping from the commitment facts.
fn create_preimage_mapping(
    commitment_facts: &HashMap<HashOutput, Vec<Felt>>,
) -> Result<PreimageMap, PatriciaError> {
    let mut preimage_mapping = PreimageMap::new();
    for (hash, raw_preimage) in commitment_facts.iter() {
        #[allow(clippy::as_conversions)]
        match raw_preimage.len() as u8 {
            Preimage::BINARY_LENGTH => {
                let binary_data = BinaryData {
                    left_hash: HashOutput(raw_preimage[0]),
                    right_hash: HashOutput(raw_preimage[1]),
                };
                preimage_mapping.insert(*hash, Preimage::Binary(binary_data));
            }
            Preimage::EDGE_LENGTH => {
                let (length, path, bottom) = (raw_preimage[0], raw_preimage[1], raw_preimage[2]);
                let edge_data = EdgeData {
                    bottom_hash: HashOutput(bottom),
                    path_to_bottom: PathToBottom::new(
                        path.into(),
                        EdgePathLength::new(length.try_into().expect("Invalid path length."))?,
                    )?,
                };
                preimage_mapping.insert(*hash, Preimage::Edge(edge_data));
            }
            _ => {
                return Err(PatriciaError::InvalidRawPreimage(raw_preimage.clone()));
            }
        }
    }
    Ok(preimage_mapping)
}
