use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::CommitmentType;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, TrieCachedStorage};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbKey, DbValue, Storage};
use tracing::info;

use crate::block_committer::input::{
    contract_address_into_node_index,
    try_node_index_into_contract_address,
    StarknetStorageValue,
};
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::hash_function::hash::ForestHashFunction;
use crate::patricia_merkle_tree::leaf::leaf_impl::{ContractState, ContractStateInput};
use crate::patricia_merkle_tree::types::{
    ClassesTrie,
    CompiledClassHash,
    ContractsTrie,
    StorageTrieMap,
};

pub struct FilledForest {
    pub storage_tries: StorageTrieMap,
    pub contracts_trie: ContractsTrie,
    pub classes_trie: ClassesTrie,
}

impl FilledForest {
    /// Writes the node serialization of the filled trees to storage. Returns the number of new
    /// objects written to storage.
    pub fn write_to_storage(&self, storage: &mut impl Storage) -> usize {
        // Serialize all trees to one hash map.
        let new_db_objects: DbHashMap = self
            .storage_tries
            .values()
            .flat_map(|tree| tree.serialize().into_iter())
            .chain(self.contracts_trie.serialize())
            .chain(self.classes_trie.serialize())
            .collect();

        // Store the new hash map.
        let n_new_facts = new_db_objects.len();
        storage
            .mset(new_db_objects)
            .unwrap_or_else(|_| panic!("Write of {n_new_facts} new facts to storage failed"));
        n_new_facts
    }

    pub fn write_to_cached_storage(&self, storage: &mut impl TrieCachedStorage) -> usize {
        let new_cached_db_objects: Vec<(CommitmentType, NodeIndex, DbKey, DbValue)> = self
            .storage_tries
            .iter()
            .flat_map(|(address, tree)| {
                tree.serialize_with_indices().into_iter().map(|((index, key), value)| {
                    (CommitmentType::Contract(*address), index, key, value)
                })
            })
            .chain(
                self.contracts_trie
                    .serialize_with_indices()
                    .into_iter()
                    .map(|((index, key), value)| (CommitmentType::State, index, key, value)),
            )
            .chain(
                self.classes_trie
                    .serialize_with_indices()
                    .into_iter()
                    .map(|((index, key), value)| (CommitmentType::Class, index, key, value)),
            )
            .collect();

        let n_new_facts = new_cached_db_objects.len();
        storage
            .mset_with_cache(new_cached_db_objects)
            .unwrap_or_else(|_| panic!("Write of {n_new_facts} new facts to storage failed"));
        n_new_facts
    }

    pub fn get_contract_root_hash(&self) -> HashOutput {
        self.contracts_trie.get_root_hash()
    }

    pub fn get_compiled_class_root_hash(&self) -> HashOutput {
        self.classes_trie.get_root_hash()
    }

    /// Creates a filled forest. Assumes the storage updates and the updated skeletons of the
    /// storage tries include all modified contracts, including those with unmodified storage.
    pub(crate) async fn create<TH: ForestHashFunction + 'static>(
        updated_forest: UpdatedSkeletonForest,
        storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: LeafModifications<CompiledClassHash>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<Self> {
        let classes_trie_task = tokio::spawn(ClassesTrie::create_with_existing_leaves::<TH>(
            updated_forest.classes_trie,
            classes_updates,
        ));

        let contracts_trie_task = tokio::task::spawn(ContractsTrie::create::<TH>(
            updated_forest.contracts_trie,
            FilledForest::get_contracts_trie_leaf_input(
                original_contracts_trie_leaves,
                storage_updates,
                updated_forest.storage_tries,
                address_to_class_hash,
                address_to_nonce,
            )?,
        ));

        let classes_trie = classes_trie_task.await?.map_err(ForestError::ClassesTrie)?;
        info!(
            "Classes trie update complete; {:?} new facts computed.",
            classes_trie.tree_map.len()
        );
        let (contracts_trie, storage_tries) =
            contracts_trie_task.await?.map_err(ForestError::ContractsTrie)?;
        info!(
            "Contracts trie update complete; {:?} new facts computed.",
            contracts_trie.tree_map.len()
        );

        Ok(Self {
            storage_tries: storage_tries
                .into_iter()
                .map(|(node_index, storage_trie)| {
                    (
                        try_node_index_into_contract_address(&node_index).unwrap_or_else(|error| {
                            panic!(
                                "Got the following error when trying to convert node index \
                                 {node_index:?} to a contract address: {error:?}",
                            )
                        }),
                        storage_trie,
                    )
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
        mut contract_address_to_storage_skeleton: HashMap<ContractAddress, UpdatedSkeletonTreeImpl>,
        address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> ForestResult<HashMap<NodeIndex, ContractStateInput>> {
        let mut leaf_index_to_leaf_input = HashMap::new();
        assert_eq!(
            contract_address_to_storage_updates.len(),
            contract_address_to_storage_skeleton.len(),
            "Mismatch between number of updated storage trie skeletons and number of storage \
             leaf-modification maps. Number of updated storage trie skeletons: {0:?}, number of \
             storage leaf-modification maps: {1:?}",
            contract_address_to_storage_skeleton.len(),
            contract_address_to_storage_updates.len()
        );
        // `contract_address_to_storage_updates` includes all modified contracts, even those with
        // unmodified storage, see StateDiff::actual_storage_updates().
        for (contract_address, storage_updates) in contract_address_to_storage_updates {
            let node_index = contract_address_into_node_index(&contract_address);
            let original_contract_state = original_contracts_trie_leaves
                .get(&node_index)
                .ok_or(ForestError::MissingContractCurrentState(contract_address))?;
            leaf_index_to_leaf_input.insert(
                node_index,
                ContractStateInput {
                    leaf_index: node_index,
                    nonce: *(address_to_nonce
                        .get(&contract_address)
                        .unwrap_or(&original_contract_state.nonce)),
                    class_hash: *(address_to_class_hash
                        .get(&contract_address)
                        .unwrap_or(&original_contract_state.class_hash)),
                    updated_skeleton: contract_address_to_storage_skeleton
                        .remove(&contract_address)
                        .ok_or(ForestError::MissingUpdatedSkeleton(contract_address))?,
                    storage_updates,
                },
            );
        }
        Ok(leaf_index_to_leaf_input)
    }
}
