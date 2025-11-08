use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, Storage};

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::node_serde::{
    FactLayoutFilledNode,
    PatriciaStorageLayout,
};
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::traversal::{SubTree, TraversalResult};

/// Storage abstraction for Patricia trees.
pub struct PatriciaStorage<S: Storage> {
    storage: S,
    storage_layout: PatriciaStorageLayout,
}

impl<S: Storage> PatriciaStorage<S> {
    pub fn new(storage: S, storage_layout: PatriciaStorageLayout) -> Self {
        Self { storage, storage_layout }
    }

    pub fn get_layout(&self) -> PatriciaStorageLayout {
        self.storage_layout
    }

    // TODO(Dori): Remove this function, no storage interation should be direct.
    pub fn get_storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub(crate) fn calculate_subtrees_roots<'b, L: Leaf>(
        &mut self,
        subtrees: &[SubTree<'b>],
    ) -> TraversalResult<Vec<FilledNode<L>>> {
        let mut subtrees_roots = vec![];
        let db_keys: Vec<DbKey> = subtrees
            .iter()
            .map(|subtree| {
                create_db_key(
                    subtree.get_root_prefix::<L>().into(),
                    &subtree.root_hash.0.to_bytes_be(),
                )
            })
            .collect();

        let db_vals = self.storage.mget(&db_keys.iter().collect::<Vec<&DbKey>>())?;
        for ((subtree, optional_val), db_key) in subtrees.iter().zip(db_vals.iter()).zip(db_keys) {
            let Some(val) = optional_val else { Err(StorageError::MissingKey(db_key))? };
            subtrees_roots.push(match self.storage_layout {
                PatriciaStorageLayout::Fact => {
                    FactLayoutFilledNode::deserialize(subtree.root_hash, val, subtree.is_leaf())?.0
                }
            });
        }
        Ok(subtrees_roots)
    }
}
