use std::collections::HashMap;
use std::sync::Arc;

use committer::hash::hash_trait::HashOutput;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTree;
use committer::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use committer::patricia_merkle_tree::types::NodeIndex;
use committer::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use committer::storage::storage_trait::Storage;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::hash_function::hash::ForestHashFunction;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    ClassHash, ClassesTrie, CompiledClassHash, ContractsTrie, Nonce, StorageTrieMap,
};

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
        let classes_trie_task = tokio::task::spawn(ClassesTrie::create_with_existing_leaves::<TH>(
            updated_forest.classes_trie.into(),
            classes_updates,
        ));

        let contracts_trie_task = tokio::task::spawn(ContractsTrie::create::<TH>(
            Arc::new(updated_forest.contracts_trie),
            Arc::new(FilledForest::get_contracts_trie_leaf_input(
                original_contracts_trie_leaves,
                storage_updates,
                updated_forest.storage_tries,
                address_to_class_hash,
                address_to_nonce,
            )?),
        ));
        let (contracts_trie, storage_tries) = contracts_trie_task.await??;
        let classes_trie = classes_trie_task.await??;

        Ok(Self {
            // TODO(Amos): Can this be avoided? Optimize.
            storage_tries: storage_tries
                .expect("Missing storage tries.")
                .into_iter()
                .map(|(node_index, storage_trie)| {
                    (ContractAddress::from(&node_index), storage_trie)
                })
                .collect(),
            contracts_trie,
            classes_trie,
        })
    }

    fn get_contracts_trie_leaf_input(
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        contract_address_to_storage_updates: HashMap<
            ContractAddress,
            LeafModifications<StarknetStorageValue>,
        >,
        mut contract_address_to_storage_trie: HashMap<ContractAddress, UpdatedSkeletonTreeImpl>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<HashMap<NodeIndex, <ContractState as Leaf>::I>> {
        let mut leaf_index_to_leaf_input = HashMap::new();
        assert_eq!(
            contract_address_to_storage_updates.len(),
            contract_address_to_storage_trie.len()
        );
        // `contract_address_to_storage_updates` includes all modified contracts, even those with
        // unmodified storage, see StateDiff::actual_storage_updates().
        for (contract_address, storage_updates) in contract_address_to_storage_updates {
            let node_index = NodeIndex::from(&contract_address);
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
                    contract_address_to_storage_trie
                        .remove(&contract_address)
                        .ok_or(ForestError::MissingUpdatedSkeleton(contract_address))?,
                    storage_updates,
                ),
            );
        }
        Ok(leaf_index_to_leaf_input)
    }
}
