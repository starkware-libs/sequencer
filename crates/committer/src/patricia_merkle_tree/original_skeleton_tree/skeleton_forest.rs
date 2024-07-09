use crate::block_committer::input::Config;
use crate::block_committer::input::ContractAddress;
use crate::block_committer::input::StarknetStorageValue;
use crate::block_committer::input::StateDiff;
use crate::forest_errors::ForestError;
use crate::forest_errors::ForestResult;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use crate::patricia_merkle_tree::node_data::leaf::ContractState;
use crate::patricia_merkle_tree::node_data::leaf::LeafModifications;
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonClassesTrieConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonContractsTrieConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonStorageTrieConfig;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTree;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::types::SortedLeafIndices;
use crate::storage::storage_trait::Storage;
use std::collections::HashMap;
use std::collections::HashSet;

#[cfg(test)]
#[path = "skeleton_forest_test.rs"]
pub mod skeleton_forest_test;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct OriginalSkeletonForest {
    pub(crate) classes_trie: OriginalSkeletonTreeImpl,
    pub(crate) contracts_trie: OriginalSkeletonTreeImpl,
    pub(crate) storage_tries: HashMap<ContractAddress, OriginalSkeletonTreeImpl>,
}

impl OriginalSkeletonForest {
    /// Creates an original skeleton forest that includes the storage tries of the modified contracts,
    /// the classes trie and the contracts trie. Additionally, returns the original contract states that
    /// are needed to compute the contract state tree.
    pub(crate) fn create(
        storage: impl Storage,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        state_diff: &StateDiff,
        config: &impl Config,
    ) -> ForestResult<(Self, HashMap<NodeIndex, ContractState>)>
    where
        Self: std::marker::Sized,
    {
        let accessed_addresses = state_diff.accessed_addresses();
        let (contracts_trie, original_contracts_trie_leaves) =
            Self::create_contracts_trie(&accessed_addresses, contracts_trie_root_hash, &storage)?;
        let storage_tries = Self::create_storage_tries(
            &state_diff.actual_storage_updates(),
            &original_contracts_trie_leaves,
            &storage,
            config,
        )?;
        let classes_trie = Self::create_classes_trie(
            &state_diff.actual_classes_updates(),
            classes_trie_root_hash,
            &storage,
            config,
        )?;

        Ok((
            Self {
                classes_trie,
                contracts_trie,
                storage_tries,
            },
            original_contracts_trie_leaves,
        ))
    }

    /// Creates the contracts trie original skeleton.
    /// Also returns the previous contracts state of the modified contracts.
    fn create_contracts_trie(
        accessed_addresses: &HashSet<&ContractAddress>,
        contracts_trie_root_hash: HashOutput,
        storage: &impl Storage,
    ) -> ForestResult<(OriginalSkeletonTreeImpl, HashMap<NodeIndex, ContractState>)> {
        let mut sorted_leaf_indices: Vec<NodeIndex> = accessed_addresses
            .iter()
            .map(|address| NodeIndex::from_contract_address(address))
            .collect();
        Ok(OriginalSkeletonTreeImpl::create_and_get_previous_leaves(
            storage,
            contracts_trie_root_hash,
            SortedLeafIndices::new(&mut sorted_leaf_indices),
            &OriginalSkeletonContractsTrieConfig::new(),
        )?)
    }

    fn create_storage_tries(
        actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
        storage: &impl Storage,
        config: &impl Config,
    ) -> ForestResult<HashMap<ContractAddress, OriginalSkeletonTreeImpl>> {
        let mut storage_tries = HashMap::new();
        for (address, updates) in actual_storage_updates {
            let mut sorted_leaf_indices: Vec<NodeIndex> = updates.keys().copied().collect();
            let contract_state = original_contracts_trie_leaves
                .get(&NodeIndex::from_contract_address(address))
                .ok_or(ForestError::MissingContractCurrentState(*address))?;

            let config = OriginalSkeletonStorageTrieConfig::new(
                updates,
                config.warn_on_trivial_modifications(),
            );
            let original_skeleton = OriginalSkeletonTreeImpl::create(
                storage,
                contract_state.storage_root_hash,
                SortedLeafIndices::new(&mut sorted_leaf_indices),
                &config,
            )?;
            storage_tries.insert(*address, original_skeleton);
        }
        Ok(storage_tries)
    }

    fn create_classes_trie(
        actual_classes_updates: &LeafModifications<CompiledClassHash>,
        classes_trie_root_hash: HashOutput,
        storage: &impl Storage,
        config: &impl Config,
    ) -> ForestResult<OriginalSkeletonTreeImpl> {
        let config = OriginalSkeletonClassesTrieConfig::new(
            actual_classes_updates,
            config.warn_on_trivial_modifications(),
        );
        let mut sorted_leaf_indices: Vec<NodeIndex> =
            actual_classes_updates.keys().copied().collect();

        Ok(OriginalSkeletonTreeImpl::create(
            storage,
            classes_trie_root_hash,
            SortedLeafIndices::new(&mut sorted_leaf_indices),
            &config,
        )?)
    }
}
