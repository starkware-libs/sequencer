use starknet_api::hash::HashOutput;

use crate::patricia_merkle_tree::node_data::inner_node::PathToBottom;

/// A node in the structure of a Patricia-Merkle tree, after the update.
///
/// `n_new_hashes` is the number of node hashes that must be computed for the
/// subtree rooted at this node during the filled-tree pass.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatedSkeletonNode {
    Binary { n_new_hashes: usize },
    Edge { path_to_bottom: PathToBottom, n_new_hashes: usize },
    // Represents a root of a subtree where none of it's descendants has changed.
    UnmodifiedSubTree(HashOutput),
    Leaf,
}
