use starknet_patricia_storage::db_object::DBObject;
use starknet_patricia_storage::errors::StorageError;
use starknet_patricia_storage::storage_trait::{
    create_db_key,
    DbHashMap,
    DbKey,
    DbValue,
    PatriciaStorageError,
    PatriciaStorageResult,
    Storage,
};

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::filled_tree::node_serde::{
    FactLayoutFilledNode,
    PatriciaStorageLayout,
};
use crate::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::traversal::{SubTree, TraversalResult};

/// Storage abstraction for Patricia trees.
pub struct PatriciaStorage<S: Storage> {
    storage: S,
    storage_layout: PatriciaStorageLayout,

    // Staging area for DB writes. We use an Option to allow taking ownership in [Self::commit].
    staged_writes: Option<DbHashMap>,
}

impl<S: Storage> PatriciaStorage<S> {
    pub fn new(storage: S, storage_layout: PatriciaStorageLayout) -> Self {
        Self { storage, storage_layout, staged_writes: None }
    }

    /// Closes the storage and returns the underlying storage.
    pub fn close(self) -> S {
        self.storage
    }

    pub fn get_layout(&self) -> PatriciaStorageLayout {
        self.storage_layout
    }

    pub fn get_storage_stats(&self) -> Option<String> {
        self.storage.get_stats()
    }

    pub fn n_staged_writes(&self) -> usize {
        self.staged_writes.as_ref().map(|map| map.len()).unwrap_or(0)
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

    fn stage_write(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        match self.staged_writes {
            Some(ref mut staged_writes) => {
                match staged_writes.get(&key) {
                    Some(existing_value) if existing_value != &value => {
                        return Err(PatriciaStorageError::KeyAlreadyExists {
                            key,
                            existing_value: existing_value.clone(),
                            new_value: value,
                        });
                    }
                    // Allow overwriting the same value.
                    Some(_) => (),
                    None => {
                        staged_writes.insert(key, value);
                    }
                }
            }
            None => self.staged_writes = Some(DbHashMap::from([(key, value)])),
        }
        Ok(())
    }

    /// Serializes the input tree into memory. Call [Self::commit] to actually write to storage.
    pub fn stage_writes<L: Leaf + 'static>(
        &mut self,
        tree: &FilledTreeImpl<L>,
    ) -> PatriciaStorageResult<()> {
        // This function iterates over each node in the tree, using the node's `db_key` as the
        // hashmap key and the result of the node's `serialize` method as the value.
        let new_writes: DbHashMap = tree
            .get_all_nodes()
            .values()
            .map(|node| match self.storage_layout {
                PatriciaStorageLayout::Fact => {
                    let fact_node = FactLayoutFilledNode(node.clone());
                    (fact_node.db_key(), fact_node.serialize())
                }
            })
            .collect();

        for (key, value) in new_writes {
            self.stage_write(key, value)?;
        }

        Ok(())
    }

    pub fn commit(&mut self) -> PatriciaStorageResult<()> {
        if let Some(map) = self.staged_writes.take() {
            self.storage.mset(map)?;
        }
        Ok(())
    }
}
