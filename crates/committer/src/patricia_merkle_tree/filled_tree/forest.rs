use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use crate::patricia_merkle_tree::node_data::leaf::{LeafData, LeafDataImpl};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::storage_trait::Storage;
use std::collections::HashMap;

pub(crate) trait FilledForest<L: LeafData> {
    #[allow(dead_code)]
    /// Serialize each tree and store it.
    fn write_to_storage(&self, storage: &mut impl Storage);
    #[allow(dead_code)]
    fn get_compiled_class_root_hash(&self) -> Result<HashOutput, FilledTreeError<L>>;
    #[allow(dead_code)]
    fn get_contract_root_hash(&self) -> Result<HashOutput, FilledTreeError<L>>;
}

pub(crate) struct FilledForestImpl {
    storage_trees: HashMap<NodeIndex, FilledTreeImpl>,
    contract_tree: FilledTreeImpl,
    compiled_class_tree: FilledTreeImpl,
}

impl FilledForest<LeafDataImpl> for FilledForestImpl {
    #[allow(dead_code)]
    fn write_to_storage(&self, storage: &mut impl Storage) {
        // Serialize all trees to one hash map.
        let new_db_objects = self
            .storage_trees
            .values()
            .flat_map(|tree| tree.serialize().into_iter())
            .chain(self.contract_tree.serialize())
            .chain(self.compiled_class_tree.serialize())
            .collect();

        // Store the new hash map
        storage.mset(new_db_objects);
    }

    fn get_contract_root_hash(&self) -> Result<HashOutput, FilledTreeError<LeafDataImpl>> {
        self.contract_tree.get_root_hash()
    }

    fn get_compiled_class_root_hash(&self) -> Result<HashOutput, FilledTreeError<LeafDataImpl>> {
        self.compiled_class_tree.get_root_hash()
    }
}
