use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::forest_errors::{ForestError, ForestResult};
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, Nonce};
use crate::patricia_merkle_tree::filled_tree::tree::FilledTree;
use crate::patricia_merkle_tree::filled_tree::tree::{
    ClassesTrie, ContractsTrie, StorageTrie, StorageTrieMap,
};
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, LeafModifications};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::ForestHashFunction;
use crate::patricia_merkle_tree::updated_skeleton_tree::skeleton_forest::UpdatedSkeletonForestImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTree;
use crate::storage::storage_trait::Storage;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;

pub trait FilledForest {
    /// Serialize each tree and store it.
    fn write_to_storage(&self, storage: &mut impl Storage);

    fn get_compiled_class_root_hash(&self) -> HashOutput;

    fn get_contract_root_hash(&self) -> HashOutput;
}

pub struct FilledForestImpl {
    pub storage_tries: StorageTrieMap,
    pub contracts_trie: ContractsTrie,
    pub classes_trie: ClassesTrie,
}

impl FilledForest for FilledForestImpl {
    fn write_to_storage(&self, storage: &mut impl Storage) {
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

    fn get_contract_root_hash(&self) -> HashOutput {
        self.contracts_trie.get_root_hash()
    }

    fn get_compiled_class_root_hash(&self) -> HashOutput {
        self.classes_trie.get_root_hash()
    }
}

impl FilledForestImpl {
    pub(crate) async fn create<
        T: UpdatedSkeletonTree + 'static,
        TH: ForestHashFunction + 'static,
    >(
        mut updated_forest: UpdatedSkeletonForestImpl<T>,
        storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: LeafModifications<CompiledClassHash>,
        current_contracts_trie_leaves: &HashMap<ContractAddress, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<Self> {
        let classes_trie =
            ClassesTrie::create::<TH>(updated_forest.classes_trie, Arc::new(classes_updates))
                .await?;

        let mut contracts_trie_modifications = HashMap::new();
        let mut filled_storage_tries = HashMap::new();
        let mut tasks = JoinSet::new();

        for (address, inner_updates) in storage_updates {
            let updated_storage_trie = updated_forest
                .storage_tries
                .remove(&address)
                .ok_or(ForestError::MissingUpdatedSkeleton(address))?;

            let old_contract_state = current_contracts_trie_leaves
                .get(&address)
                .ok_or(ForestError::MissingContractCurrentState(address))?;
            tasks.spawn(Self::new_contract_state::<T, TH>(
                address,
                *(address_to_nonce
                    .get(&address)
                    .unwrap_or(&old_contract_state.nonce)),
                *(address_to_class_hash
                    .get(&address)
                    .unwrap_or(&old_contract_state.class_hash)),
                updated_storage_trie,
                inner_updates,
            ));
        }

        while let Some(result) = tasks.join_next().await {
            let (address, new_contract_state, filled_storage_trie) = result??;
            contracts_trie_modifications.insert(
                NodeIndex::from_contract_address(&address),
                new_contract_state,
            );
            filled_storage_tries.insert(address, filled_storage_trie);
        }

        let contracts_trie = ContractsTrie::create::<TH>(
            updated_forest.contracts_trie,
            Arc::new(contracts_trie_modifications),
        )
        .await?;

        Ok(Self {
            storage_tries: filled_storage_tries,
            contracts_trie,
            classes_trie,
        })
    }

    async fn new_contract_state<
        T: UpdatedSkeletonTree + 'static,
        TH: ForestHashFunction + 'static,
    >(
        contract_address: ContractAddress,
        new_nonce: Nonce,
        new_class_hash: ClassHash,
        updated_storage_trie: T,
        inner_updates: LeafModifications<StarknetStorageValue>,
    ) -> ForestResult<(ContractAddress, ContractState, StorageTrie)> {
        let filled_storage_trie =
            StorageTrie::create::<TH>(updated_storage_trie, Arc::new(inner_updates)).await?;
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
