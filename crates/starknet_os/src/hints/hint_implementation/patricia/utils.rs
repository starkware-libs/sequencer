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
    pub fn length(&self) -> u8 {
        match self {
            Preimage::Binary(_) => 2,
            Preimage::Edge(_) => 3,
        }
    }

    pub(crate) fn get_binary(&self) -> Result<&BinaryData, PatriciaError> {
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
    pub(crate) fn get_children(&self) -> (&UpdateTree, &UpdateTree) {
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
}

impl CanonicNode {
    fn new(preimage_map: &PreimageMap, node_hash: &HashOutput) -> CanonicNode {
        if let Some(Preimage::Edge(edge)) = preimage_map.get(node_hash) {
            return CanonicNode::Edge(*edge);
        }
        CanonicNode::BinaryOrLeaf(*node_hash)
    }

    fn get_edge(&self) -> Result<&EdgeData, PatriciaError> {
        match self {
            CanonicNode::Edge(edge) => Ok(edge),
            CanonicNode::BinaryOrLeaf(_) => Err(PatriciaError::ExpectedEdge(self.clone())),
        }
    }

    /// Returns the hash of the node. If the node is an EdgeNode, returns the bottom hash.
    fn get_hash(&self) -> &HashOutput {
        match self {
            CanonicNode::BinaryOrLeaf(hash) => hash,
            CanonicNode::Edge(edge) => &edge.bottom_hash,
        }
    }

    fn is_edge(&self) -> bool {
        matches!(self, CanonicNode::Edge(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicChildren {
    left: Option<CanonicNode>,
    right: Option<CanonicNode>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Path(pub(crate) PathToBottom);

impl Path {
    fn turn_left(&self) -> Result<Self, PatriciaError> {
        let path_to_bottom = PathToBottom::new(
            EdgePath(self.0.path.0 << 1),
            EdgePathLength::new(u8::from(self.0.length) + 1u8)?,
        )?;

        Ok(Self(path_to_bottom))
    }

    fn turn_right(&self) -> Result<Self, PatriciaError> {
        let path_to_bottom = PathToBottom::new(
            EdgePath((self.0.path.0 << 1) + 1u128),
            EdgePathLength::new(u8::from(self.0.length) + 1u8)?,
        )?;

        Ok(Self(path_to_bottom))
    }

    /// Remove n_edges from the beginning of the path and return the new path.
    fn remove_first_edges(&self, n_edges: EdgePathLength) -> Result<Self, PatriciaError> {
        let new_path_to_bottom = self.0.remove_first_edges(n_edges)?;
        Ok(Self(new_path_to_bottom))
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

/// Retrieves the children of a CanonicNode.
/// We call this function only from `preimage_tree`, which stops when we get to a leaf.
/// So we assume node is not a leaf.
fn get_children(
    node: &CanonicNode,
    preimage_map: &PreimageMap,
) -> Result<CanonicChildren, PatriciaError> {
    let node_hash = node.get_hash();

    if !node.is_edge() {
        let left;
        let right;

        if node_hash.0 == Felt::ZERO {
            // An empty node.
            (left, right) = (HashOutput(Felt::ZERO), HashOutput(Felt::ZERO));
        } else {
            // A binary node (not a leaf).
            let preimage =
                preimage_map.get(node_hash).ok_or(PatriciaError::MissingPreimage(*node_hash))?;

            let binary = preimage.get_binary()?;
            left = binary.left_hash;
            right = binary.right_hash;
        }
        Ok(CanonicChildren {
            left: Some(CanonicNode::new(preimage_map, &left)),
            right: Some(CanonicNode::new(preimage_map, &right)),
        })
    } else {
        let edge_data = node.get_edge()?;
        let path_to_bottom = edge_data.path_to_bottom;

        let child = match path_to_bottom.length.into() {
            1 => CanonicNode::BinaryOrLeaf(*node_hash),
            _ => {
                let new_path = path_to_bottom.remove_first_edges(EdgePathLength::new(1)?)?;
                CanonicNode::Edge(EdgeData { bottom_hash: *node_hash, path_to_bottom: new_path })
            }
        };

        if path_to_bottom.is_left_descendant() {
            return Ok(CanonicChildren { left: Some(child), right: None });
        }
        Ok(CanonicChildren { left: None, right: Some(child) })
    }
}

#[derive(Debug)]
enum PreimageNode<'pm> {
    Leaf,
    Branch { left: Option<PreimageNodeIterator<'pm>>, right: Option<PreimageNodeIterator<'pm>> },
}

/// Builds the children of the root node lazily.
#[derive(Debug)]
struct PreimageNodeIterator<'pm> {
    height: SubTreeHeight,
    preimage_map: &'pm PreimageMap,
    node: CanonicNode,
    is_done: bool,
}

impl<'pm> PreimageNodeIterator<'pm> {
    fn new(height: SubTreeHeight, preimage_map: &'pm PreimageMap, node: CanonicNode) -> Self {
        Self { height, preimage_map, node, is_done: false }
    }

    fn get_children(
        &mut self,
    ) -> Result<(Option<PreimageNodeIterator<'pm>>, Option<PreimageNodeIterator<'pm>>), PatriciaError>
    {
        match self.next() {
            Some(Ok(PreimageNode::Branch { left, right })) => Ok((left, right)),
            Some(Ok(PreimageNode::Leaf)) => Err(PatriciaError::ExpectedBranch("Leaf".to_string())),
            Some(Err(e)) => Err(e),
            None => unreachable!("Iterator should not be empty."),
        }
    }
}

impl<'pm> Iterator for PreimageNodeIterator<'pm> {
    type Item = Result<PreimageNode<'pm>, PatriciaError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }
        self.is_done = true;

        // Check for children
        if self.height.0 == 0 {
            return Some(Ok(PreimageNode::Leaf));
        }
        let children = match get_children(&self.node, self.preimage_map) {
            Ok(children) => children,
            Err(e) => return Some(Err(e)),
        };

        let left_child = match children.left {
            None => None,
            Some(left) => Some(PreimageNodeIterator::new(self.height - 1, self.preimage_map, left)),
        };
        let right_child = match children.right {
            None => None,
            Some(right) => {
                Some(PreimageNodeIterator::new(self.height - 1, self.preimage_map, right))
            }
        };

        Some(Ok(PreimageNode::Branch { left: left_child, right: right_child }))
    }
}

/// Builds a tree structure similar to build_update_tree(), from a root hash, and a preimage
/// dictionary.
/// The Python implementation returns a generator as follows:
/// * if node is a leaf: [0]
/// * Otherwise: [left, right] where each child is either None if empty or a generator defined
///   recursively.
///
/// Note that this does not necessarily traverse the entire tree. The caller may open the branches
/// as they wish.
fn preimage_tree(
    height: SubTreeHeight,
    preimage_map: &PreimageMap,
    node: CanonicNode,
) -> PreimageNodeIterator<'_> {
    PreimageNodeIterator::new(height, preimage_map, node)
}
