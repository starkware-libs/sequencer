use std::collections::{HashMap, HashSet};

use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
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
    pub(crate) const BINARY_LENGTH: u8 = 2;
    pub(crate) const EDGE_LENGTH: u8 = 3;

    pub fn length(&self) -> u8 {
        match self {
            Self::Binary(_) => Self::BINARY_LENGTH,
            Self::Edge(_) => Self::EDGE_LENGTH,
        }
    }

    pub(crate) fn get_binary(&self) -> Result<&BinaryData, PatriciaError> {
        match self {
            Self::Binary(binary) => Ok(binary),
            Self::Edge(_) => Err(PatriciaError::ExpectedBinary(self.clone())),
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
    pub(crate) fn get_children(&self) -> (&UpdateTree, &UpdateTree) {
        match self {
            Self::Left(left) => (left, &UpdateTree::None),
            Self::Right(right) => (&UpdateTree::None, right),
            Self::Both(left, right) => (left, right),
        }
    }

    pub(crate) fn case(&self) -> DecodeNodeCase {
        match self {
            Self::Left(_) => DecodeNodeCase::Left,
            Self::Right(_) => DecodeNodeCase::Right,
            Self::Both(_, _) => DecodeNodeCase::Both,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateTree {
    InnerNode(InnerNode),
    Leaf(HashOutput),
    // Represents a node where none of it's descendants has been modified.
    None,
}

type TreeLayer = HashMap<LayerIndex, UpdateTree>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicNode {
    BinaryOrLeaf(HashOutput),
    Edge(EdgeData),
    Empty,
}

impl CanonicNode {
    fn new(preimage_map: &PreimageMap, node_hash: &HashOutput) -> CanonicNode {
        if node_hash.0 == Felt::ZERO {
            return Self::Empty;
        }
        if let Some(Preimage::Edge(edge)) = preimage_map.get(node_hash) {
            return Self::Edge(*edge);
        }
        Self::BinaryOrLeaf(*node_hash)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Path(pub(crate) PathToBottom);

impl Path {
    fn turn(&self, right: bool) -> Result<Self, PatriciaError> {
        Ok(Self(PathToBottom::new(
            EdgePath((self.0.path.0 << 1) + u128::from(right)),
            EdgePathLength::new(u8::from(self.0.length) + 1u8)?,
        )?))
    }

    fn turn_left(&self) -> Result<Self, PatriciaError> {
        self.turn(false)
    }

    fn turn_right(&self) -> Result<Self, PatriciaError> {
        self.turn(true)
    }

    /// Remove n_edges from the beginning of the path and return the new path.
    fn remove_first_edges(&self, n_edges: EdgePathLength) -> Result<Self, PatriciaError> {
        Ok(Self(self.0.remove_first_edges(n_edges)?))
    }
}

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
pub(crate) fn create_preimage_mapping(
    commitment_facts: &HashMap<HashOutput, Vec<Felt>>,
) -> Result<PreimageMap, PatriciaError> {
    let mut preimage_mapping = PreimageMap::new();
    for (hash, raw_preimage) in commitment_facts.iter() {
        match raw_preimage.as_slice() {
            [left, right] => {
                let binary_data =
                    BinaryData { left_hash: HashOutput(*left), right_hash: HashOutput(*right) };
                preimage_mapping.insert(*hash, Preimage::Binary(binary_data));
            }
            [length, path, bottom] => {
                let edge_data = EdgeData {
                    bottom_hash: HashOutput(*bottom),
                    path_to_bottom: PathToBottom::new(
                        (*path).into(),
                        EdgePathLength::new((*length).try_into().map_err(|_| {
                            PatriciaError::InvalidRawPreimage(raw_preimage.clone())
                        })?)?,
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

/// Retrieves the children of a CanonicNode.
/// We call this function only from `get_descents`, which stops when we get to a leaf.
/// So we assume node is not a leaf.
fn get_children(
    node: &CanonicNode,
    preimage_map: &PreimageMap,
) -> Result<(CanonicNode, CanonicNode), PatriciaError> {
    match node {
        CanonicNode::Empty => {
            // An empty node.
            Ok((CanonicNode::Empty, CanonicNode::Empty))
        }
        CanonicNode::BinaryOrLeaf(hash) => {
            // A binary node (not a leaf).
            let preimage = preimage_map.get(hash).ok_or(PatriciaError::MissingPreimage(*hash))?;

            let binary = preimage.get_binary()?;
            Ok((
                CanonicNode::new(preimage_map, &binary.left_hash),
                CanonicNode::new(preimage_map, &binary.right_hash),
            ))
        }
        CanonicNode::Edge(edge) => {
            let hash = edge.bottom_hash;
            let path_to_bottom = edge.path_to_bottom;

            let child = if u8::from(path_to_bottom.length) == 1 {
                CanonicNode::BinaryOrLeaf(hash)
            } else {
                let new_path = path_to_bottom.remove_first_edges(EdgePathLength::new(1)?)?;
                CanonicNode::Edge(EdgeData { bottom_hash: hash, path_to_bottom: new_path })
            };

            if path_to_bottom.is_left_descendant() {
                return Ok((child, CanonicNode::Empty));
            }
            Ok((CanonicNode::Empty, child))
        }
    }
}
