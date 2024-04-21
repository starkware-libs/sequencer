use std::collections::HashMap;
use std::collections::HashSet;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::types::{LeafDataTrait, NodeIndex};
use crate::storage::storage_trait::Storage;
use crate::storage::storage_trait::StorageKey;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub(crate) trait FilledTree<L: LeafDataTrait> {
    /// Serializes the tree into storage. Returns hash set of keys of the serialized nodes,
    /// if successful.
    fn serialize(&self, storage: &mut impl Storage)
        -> Result<HashSet<StorageKey>, FilledTreeError>;
    fn get_all_nodes(&self) -> &HashMap<NodeIndex, FilledNode<L>>;
    fn get_root_hash(&self) -> Result<HashOutput, FilledTreeError>;
}

pub(crate) struct FilledTreeImpl<L: LeafDataTrait> {
    tree_map: HashMap<NodeIndex, FilledNode<L>>,
}

impl<L: LeafDataTrait> FilledTreeImpl<L> {
    pub(crate) fn new(tree_map: HashMap<NodeIndex, FilledNode<L>>) -> Self {
        Self { tree_map }
    }
}

impl<L: LeafDataTrait> FilledTree<L> for FilledTreeImpl<L> {
    fn serialize(
        &self,
        _storage: &mut impl Storage,
    ) -> Result<HashSet<StorageKey>, FilledTreeError> {
        todo!()
    }

    fn get_all_nodes(&self) -> &HashMap<NodeIndex, FilledNode<L>> {
        &self.tree_map
    }

    fn get_root_hash(&self) -> Result<HashOutput, FilledTreeError> {
        match self.tree_map.get(&NodeIndex::root_index()) {
            Some(root_node) => Ok(root_node.hash),
            None => Err(FilledTreeError::MissingRoot),
        }
    }
}

pub(crate) type FilledTreeResult<T> = Result<T, FilledTreeError>;
