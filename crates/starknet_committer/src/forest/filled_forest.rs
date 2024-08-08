use std::collections::HashMap;
use std::sync::Arc;

use committer::hash::hash_trait::HashOutput;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTree;
use committer::patricia_merkle_tree::node_data::leaf::LeafModifications;
use committer::patricia_merkle_tree::types::NodeIndex;
use committer::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use committer::storage::storage_trait::Storage;
use tokio::task::JoinSet;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::hash_function::hash::ForestHashFunction;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    ClassHash,
    ClassesTrie,
    CompiledClassHash,
    ContractsTrie,
    Nonce,
    StorageTrie,
    StorageTrieMap,
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
        mut updated_forest: UpdatedSkeletonForest,
        storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: LeafModifications<CompiledClassHash>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<Self> {
        let classes_trie_task = tokio::spawn(ClassesTrie::create::<TH>(
            Arc::new(updated_forest.classes_trie),
            Arc::new(classes_updates),
        ));
        let mut contracts_trie_modifications = HashMap::new();
        let mut filled_storage_tries = HashMap::new();
        let mut contracts_state_tasks = JoinSet::new();

        for (address, inner_updates) in storage_updates {
            let updated_storage_trie = updated_forest
                .storage_tries
                .remove(&address)
                .ok_or(ForestError::MissingUpdatedSkeleton(address))?;

            let original_contract_state = original_contracts_trie_leaves
                .get(&((&address).into()))
                .ok_or(ForestError::MissingContractCurrentState(address))?;
            contracts_state_tasks.spawn(Self::new_contract_state::<TH>(
                address,
                *(address_to_nonce.get(&address).unwrap_or(&original_contract_state.nonce)),
                *(address_to_class_hash
                    .get(&address)
                    .unwrap_or(&original_contract_state.class_hash)),
                updated_storage_trie,
                inner_updates,
            ));
        }

        while let Some(result) = contracts_state_tasks.join_next().await {
            let (address, new_contract_state, filled_storage_trie) = result??;
            contracts_trie_modifications.insert((&address).into(), new_contract_state);
            filled_storage_tries.insert(address, filled_storage_trie);
        }

        let contracts_trie_task = tokio::spawn(ContractsTrie::create::<TH>(
            Arc::new(updated_forest.contracts_trie),
            Arc::new(contracts_trie_modifications),
        ));

        Ok(Self {
            storage_tries: filled_storage_tries,
            contracts_trie: contracts_trie_task.await??,
            classes_trie: classes_trie_task.await??,
        })
    }

    async fn new_contract_state<TH: ForestHashFunction + 'static>(
        contract_address: ContractAddress,
        new_nonce: Nonce,
        new_class_hash: ClassHash,
        updated_storage_trie: UpdatedSkeletonTreeImpl,
        inner_updates: LeafModifications<StarknetStorageValue>,
    ) -> ForestResult<(ContractAddress, ContractState, StorageTrie)> {
        let filled_storage_trie =
            StorageTrie::create::<TH>(Arc::new(updated_storage_trie), Arc::new(inner_updates))
                .await?;
        let new_root_hash = filled_storage_trie.get_root_hash();
        Ok((
            contract_address,
            ContractState {
                nonce: new_nonce,
                storage_root_hash: new_root_hash,
                class_hash: new_class_hash,
            },
            filled_storage_trie,
        ))
    }
}
