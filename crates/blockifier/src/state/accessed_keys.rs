use std::collections::BTreeSet;
#[cfg(any(feature = "testing", test))]
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::StorageKey;

use super::cached_state::{CommitmentStateDiff, StateChangesKeys, StorageEntry};
use super::stateful_compression::predicted_alias_storage_entries;
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::transaction::objects::TransactionExecutionInfo;

#[cfg(test)]
#[path = "accessed_keys_test.rs"]
pub mod accessed_keys_test;

/// The trie-leaf input that the OS needs to read at the execution of a block.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccessedKeys {
    /// Storage-trie leaves: `(contract_address, storage_key)` visited during execution, written in
    /// the state diff, the `(BLOCK_HASH_TABLE_ADDRESS, block_number)` entries for SNOS ProofFacts,
    /// and (when enabled) the alias-contract entries that `allocate_aliases_in_storage` will
    /// touch.
    pub storage_keys: BTreeSet<StorageEntry>,
    /// Contracts-trie leaves: every contract address whose leaf the OS reads (call targets,
    /// delegate-call targets, replaced classes, contracts written in the state diff).
    pub accessed_contracts: BTreeSet<ContractAddress>,
    /// Contract-class-trie leaves: every class hash whose compiled-class-hash leaf the OS reads
    /// (class hashes dispatched to, newly deployed class hashes, declarations).
    pub accessed_class_hashes: BTreeSet<ClassHash>,
}

#[cfg(any(feature = "testing", test))]
impl From<AccessedKeys> for StateChangesKeys {
    fn from(accessed_keys: AccessedKeys) -> Self {
        Self {
            nonce_keys: HashSet::new(),
            class_hash_keys: HashSet::new(),
            storage_keys: accessed_keys.storage_keys.into_iter().collect(),
            compiled_class_hash_keys: accessed_keys.accessed_class_hashes.into_iter().collect(),
            modified_contracts: accessed_keys.accessed_contracts.into_iter().collect(),
        }
    }
}

impl From<StateChangesKeys> for AccessedKeys {
    fn from(state_changes_keys: StateChangesKeys) -> Self {
        Self {
            storage_keys: state_changes_keys.storage_keys.into_iter().collect(),
            accessed_contracts: state_changes_keys.modified_contracts.into_iter().collect(),
            accessed_class_hashes: state_changes_keys
                .compiled_class_hash_keys
                .into_iter()
                .collect(),
        }
    }
}

impl AccessedKeys {
    /// The total number of accessed trie leaves across all three tries.
    pub fn len(&self) -> usize {
        self.storage_keys.len() + self.accessed_contracts.len() + self.accessed_class_hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Builds the [`AccessedKeys`] the OS needs to read at the execution of a block.
    pub fn new<'a>(
        execution_infos: impl IntoIterator<Item = &'a TransactionExecutionInfo>,
        proof_facts_block_numbers: impl IntoIterator<Item = &'a BlockNumber>,
        state_diff: &CommitmentStateDiff,
        versioned_constants: &VersionedConstants,
    ) -> Self {
        let mut storage_keys: BTreeSet<StorageEntry> = BTreeSet::new();
        let mut accessed_contracts: BTreeSet<ContractAddress> = BTreeSet::new();
        let mut accessed_class_hashes: BTreeSet<ClassHash> = BTreeSet::new();

        // Scan the call infos.
        for execution_info in execution_infos {
            for call_info in execution_info.non_optional_call_infos().flat_map(|ci| ci.iter()) {
                storage_keys.extend(call_info.get_visited_storage_entries());
                storage_keys.extend(call_info.storage_access_tracker.accessed_blocks.iter().map(
                    |block_number| (BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number.0)),
                ));
                accessed_contracts.extend(call_info.get_visited_contract_addresses());
                if let Some(class_hash) = call_info.call.class_hash {
                    accessed_class_hashes.insert(class_hash);
                }
            }
        }

        // Storage entries written in the state diff.
        for (address, inner) in &state_diff.storage_updates {
            storage_keys.extend(inner.keys().map(|key| (*address, *key)));
        }
        // Add the block hash table entries for the proof facts.
        for block_number in proof_facts_block_numbers {
            storage_keys.insert((BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number.0)));
        }
        // Add the alias contract entries when stateful compression is enabled (matching the
        // condition under which `allocate_aliases_in_storage` runs in finalization).
        if versioned_constants.enable_stateful_compression {
            let alias_contract_address =
                versioned_constants.os_constants.os_contract_addresses.alias_contract_address();
            storage_keys
                .extend(predicted_alias_storage_entries(state_diff, alias_contract_address));
        }

        accessed_contracts.extend(storage_keys.iter().map(|(address, _)| *address));
        accessed_contracts.extend(state_diff.get_contract_addresses().into_iter().copied());

        accessed_class_hashes.extend(state_diff.address_to_class_hash.values().copied());
        accessed_class_hashes.extend(state_diff.class_hash_to_compiled_class_hash.keys().copied());

        Self { storage_keys, accessed_contracts, accessed_class_hashes }
    }
}
