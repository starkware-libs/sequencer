use std::collections::HashMap;
use std::sync::Arc;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::forest_errors::{ForestError, ForestResult};
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::filled_tree::tree::{
    ClassesTrie, ContractsTrie, FilledTree, StorageTrieMap,
};
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, Leaf, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::ForestHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::skeleton_forest::UpdatedSkeletonForest;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::storage::storage_trait::Storage;

pub struct FilledForest {
    pub storage_tries: StorageTrieMap,
    pub contracts_trie: ContractsTrie,
    pub classes_trie: ClassesTrie,
}

impl FilledForest {
    pub fn write_to_storage(&self, storage: &mut impl Storage) {
        // Serialize all trees to one hash map.
        let new_db_objects = self
            .storage_tries
            .values()
            .flat_map(|tree| tree.serialize().into_iter())
            .chain(self.contracts_trie.serialize())
            .chain(self.classes_trie.serialize())
            .collect();

        // Store the new hash map
        storage.mset(new_db_objects);
    }

    pub fn get_contract_root_hash(&self) -> HashOutput {
        self.contracts_trie.get_root_hash()
    }

    pub fn get_compiled_class_root_hash(&self) -> HashOutput {
        self.classes_trie.get_root_hash()
    }

    pub(crate) async fn create<TH: ForestHashFunction + 'static>(
        updated_forest: UpdatedSkeletonForest,
        storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: LeafModifications<CompiledClassHash>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<Self> {
        let classes_trie = ClassesTrie::create_no_leaf_output::<TH>(
            Arc::new(updated_forest.classes_trie),
            Arc::new(classes_updates),
        )
        .await?;

        let (contracts_trie, storage_tries) = ContractsTrie::create::<TH>(
            Arc::new(updated_forest.contracts_trie),
            Arc::new(FilledForest::get_contracts_trie_leaf_input(
                original_contracts_trie_leaves,
                storage_updates,
                updated_forest.storage_tries,
                address_to_class_hash,
                address_to_nonce,
            )?),
        )
        .await?;

        Ok(Self {
            storage_tries: storage_tries
                .unwrap_or_else(|| panic!("Missing storage tries."))
                .into_iter()
                .map(|(node_index, storage_trie)| (node_index.to_contract_address(), storage_trie))
                .collect(),
            contracts_trie,
            classes_trie,
        })
    }

    // TODO(Amos, 1/8/2024): Can this be done more efficiently?
    // should error be returned if keys are missing?
    fn get_contracts_trie_leaf_input(
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        mut storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        mut storage_tries: HashMap<ContractAddress, UpdatedSkeletonTreeImpl>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<HashMap<NodeIndex, <ContractState as Leaf>::I>> {
        let mut leaf_index_to_leaf_input = HashMap::new();
        assert_eq!(storage_updates.len(), storage_tries.len());
        // `storage_updates` includes all modified contracts, see
        // StateDiff::actual_storage_updates().
        for contract_address in storage_updates.keys().cloned().collect::<Vec<_>>() {
            let node_index = NodeIndex::from_contract_address(&contract_address);
            let original_contract_state = original_contracts_trie_leaves
                .get(&node_index)
                .ok_or(ForestError::MissingContractCurrentState(contract_address))?;
            leaf_index_to_leaf_input.insert(
                node_index,
                (
                    node_index,
                    *(address_to_nonce
                        .get(&contract_address)
                        .unwrap_or(&original_contract_state.nonce)),
                    *(address_to_class_hash
                        .get(&contract_address)
                        .unwrap_or(&original_contract_state.class_hash)),
                    storage_tries.remove(&contract_address).unwrap_or_else(|| {
                        panic!("Missing update skeleton tree for contract {:?}", contract_address)
                    }),
                    storage_updates.remove(&contract_address).unwrap_or_else(|| {
                        panic!("Missing storage updates for contract {:?}", contract_address)
                    }),
                ),
            );
        }
        Ok(leaf_index_to_leaf_input)
    }
}
