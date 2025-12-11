use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{
    create_db_key,
    DbHashMap,
    DbKey,
    DbKeyPrefix,
    Storage,
};

use crate::block_committer::input::{
    contract_address_into_node_index,
    Config,
    ConfigImpl,
    FactsDInitialRead,
    StarknetStorageValue,
};
use crate::db::create_facts_tree::{
    create_original_skeleton_tree,
    create_original_skeleton_tree_and_get_previous_leaves,
};
use crate::db::forest_trait::{ForestMetadata, ForestMetadataType, ForestReader, ForestWriter};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::{ForestError, ForestResult};
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::tree::{
    OriginalSkeletonClassesTrieConfig,
    OriginalSkeletonContractsTrieConfig,
    OriginalSkeletonStorageTrieConfig,
};
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct FactsDb<S: Storage> {
    // TODO(Yoav): Define StorageStats trait and impl it here. Then, make the storage field
    // private.
    pub storage: S,
}

impl<S: Storage> FactsDb<S> {
    pub const COMMITMENT_OFFSET_KEY: &[u8; 17] = b"commitment_offset";
    pub const STATE_DIFF_HASH_PREFIX: &[u8; 15] = b"state_diff_hash";

    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Creates the contracts trie original skeleton.
    /// Also returns the previous contracts state of the modified contracts.
    async fn create_contracts_trie<'a>(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<(OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, ContractState>)> {
        Ok(create_original_skeleton_tree_and_get_previous_leaves(
            &mut self.storage,
            contracts_trie_root_hash,
            contracts_trie_sorted_indices,
            &HashMap::new(),
            &OriginalSkeletonContractsTrieConfig::new(),
        )
        .await?)
    }

    async fn create_storage_tries<'a>(
        &mut self,
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        config: &impl Config,
        storage_tries_sorted_indices: &HashMap<ContractAddress, SortedLeafIndices<'a>>,
    ) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>> {
        let mut storage_tries = HashMap::new();
        for (address, updates) in actual_storage_updates {
            let sorted_leaf_indices = storage_tries_sorted_indices
                .get(address)
                .ok_or(ForestError::MissingSortedLeafIndices(*address))?;
            let contract_state = original_contracts_trie_leaves
                .get(&contract_address_into_node_index(address))
                .ok_or(ForestError::MissingContractCurrentState(*address))?;
            let config =
                OriginalSkeletonStorageTrieConfig::new(config.warn_on_trivial_modifications());

            let original_skeleton = create_original_skeleton_tree(
                &mut self.storage,
                contract_state.storage_root_hash,
                *sorted_leaf_indices,
                &config,
                updates,
            )
            .await?;
            storage_tries.insert(*address, original_skeleton);
        }
        Ok(storage_tries)
    }

    async fn create_classes_trie<'a>(
        &mut self,
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        config: &impl Config,
        contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    ) -> ForestResult<OriginalSkeletonTreeImpl<'a>> {
        let config = OriginalSkeletonClassesTrieConfig::new(config.warn_on_trivial_modifications());

        Ok(create_original_skeleton_tree(
            &mut self.storage,
            classes_trie_root_hash,
            contracts_trie_sorted_indices,
            &config,
            actual_classes_updates,
        )
        .await?)
    }
}

impl FactsDb<MapStorage> {
    pub fn consume_storage(self) -> MapStorage {
        self.storage
    }
}

impl<'a, S: Storage> ForestReader<'a, FactsDInitialRead> for FactsDb<S> {
    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read(
        &mut self,
        context: FactsDInitialRead,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ConfigImpl,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        let (contracts_trie, original_contracts_trie_leaves) = self
            .create_contracts_trie(
                context.contracts_trie_root_hash,
                forest_sorted_indices.contracts_trie_sorted_indices,
            )
            .await?;
        let storage_tries = self
            .create_storage_tries(
                storage_updates,
                &original_contracts_trie_leaves,
                &config,
                &forest_sorted_indices.storage_tries_sorted_indices,
            )
            .await?;
        let classes_trie = self
            .create_classes_trie(
                classes_updates,
                context.classes_trie_root_hash,
                &config,
                forest_sorted_indices.classes_trie_sorted_indices,
            )
            .await?;

        Ok((
            OriginalSkeletonForest { classes_trie, contracts_trie, storage_tries },
            original_contracts_trie_leaves,
        ))
    }
}

#[async_trait]
impl<S: Storage> ForestWriter for FactsDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> DbHashMap {
        filled_forest
            .storage_tries
            .values()
            .flat_map(|tree| tree.serialize().into_iter())
            .chain(filled_forest.contracts_trie.serialize())
            .chain(filled_forest.classes_trie.serialize())
            .collect()
    }

    async fn write_updates(&mut self, updates: DbHashMap) -> usize {
        let n_updates = updates.len();
        self.storage
            .mset(updates)
            .await
            .unwrap_or_else(|_| panic!("Write of {n_updates} new updates to storage failed"));
        n_updates
    }
}

impl<S: Storage> ForestMetadata for FactsDb<S> {
    /// Returns the db key for the metadata type.
    /// The data keys in a facts DB are the result of a hash function; therefore, they are not
    /// expected to collide with these metadata keys.
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey {
        match metadata_type {
            ForestMetadataType::CommitmentOffset => DbKey(Self::COMMITMENT_OFFSET_KEY.to_vec()),
            ForestMetadataType::StateDiffHash(block_number) => {
                let state_diff_hash_key_prefix = DbKeyPrefix::new(Self::STATE_DIFF_HASH_PREFIX);
                create_db_key(state_diff_hash_key_prefix, &block_number.0.to_be_bytes())
            }
        }
    }
}
