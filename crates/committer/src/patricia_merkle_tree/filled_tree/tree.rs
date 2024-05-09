use std::collections::HashMap;
use std::collections::HashSet;

use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::storage_trait::Storage;
use crate::storage::storage_trait::StorageKey;

/// Consider a Patricia-Merkle Tree which has been updated with new leaves.
/// FilledTree consists of all nodes which were modified in the update, including their updated
/// data and hashes.
pub(crate) trait FilledTree<L: LeafData> {
    /// Serializes the tree into storage. Returns hash set of keys of the serialized nodes,
    /// if successful.
    #[allow(dead_code)]
    fn serialize(&self, storage: &mut impl Storage)
        -> Result<HashSet<StorageKey>, FilledTreeError>;
    #[allow(dead_code)]
    fn get_root_hash(&self) -> Result<HashOutput, FilledTreeError>;
}

pub(crate) struct FilledTreeImpl<L: LeafData> {
    tree_map: HashMap<NodeIndex, FilledNode<L>>,
}

impl<L: LeafData> FilledTreeImpl<L> {
    #[allow(dead_code)]
    pub(crate) fn new(tree_map: HashMap<NodeIndex, FilledNode<L>>) -> Self {
        Self { tree_map }
    }

    #[allow(dead_code)]
    pub(crate) fn get_all_nodes(&self) -> &HashMap<NodeIndex, FilledNode<L>> {
        &self.tree_map
    }
}

impl<L: LeafData> FilledTree<L> for FilledTreeImpl<L> {
    fn serialize(
        &self,
        _storage: &mut impl Storage,
    ) -> Result<HashSet<StorageKey>, FilledTreeError> {
        todo!()
    }
    fn get_root_hash(&self) -> Result<HashOutput, FilledTreeError> {
        match self.tree_map.get(&NodeIndex::ROOT) {
            Some(root_node) => Ok(root_node.hash),
            None => Err(FilledTreeError::MissingRoot),
        }
    }
}
