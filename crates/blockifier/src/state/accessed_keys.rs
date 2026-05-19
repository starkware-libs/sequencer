use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::StorageKey;

use super::cached_state::{StateChangesKeys, StateMaps, StorageEntry};
use super::stateful_compression::predicted_alias_storage_entries;
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
    pub storage_keys: HashSet<StorageEntry>,
    /// Contracts-trie leaves: every contract address whose leaf the OS reads (call targets,
    /// delegate-call targets, contracts written in the state diff).
    pub modified_contracts: HashSet<ContractAddress>,
    /// Contract-class-trie leaves: every class hash whose compiled-class-hash leaf the OS reads
    /// (class hashes dispatched to, newly deployed class hashes, declarations).
    pub compiled_class_hash_keys: HashSet<ClassHash>,
}

impl From<AccessedKeys> for StateChangesKeys {
    fn from(accessed_keys: AccessedKeys) -> Self {
        Self {
            nonce_keys: HashSet::new(),
            class_hash_keys: HashSet::new(),
            storage_keys: accessed_keys.storage_keys,
            compiled_class_hash_keys: accessed_keys.compiled_class_hash_keys,
            modified_contracts: accessed_keys.modified_contracts,
        }
    }
}

/// Builds the [`AccessedKeys`] the OS needs to read at the execution of a block.
pub fn compute_accessed_keys<'a>(
    execution_infos: impl IntoIterator<Item = &'a TransactionExecutionInfo>,
    proof_facts_block_numbers: impl IntoIterator<Item = &'a BlockNumber>,
    state_diff: &StateMaps,
    alias_contract_address: ContractAddress,
    include_alias_predictions: bool,
) -> AccessedKeys {
    let mut storage_keys: HashSet<StorageEntry> = HashSet::new();
    let mut modified_contracts: HashSet<ContractAddress> = HashSet::new();
    let mut compiled_class_hash_keys: HashSet<ClassHash> = HashSet::new();

    // Scan the call infos.
    for execution_info in execution_infos {
        for call_info in execution_info.non_optional_call_infos().flat_map(|ci| ci.iter()) {
            storage_keys.extend(call_info.get_visited_storage_entries());
            storage_keys.extend(
                call_info.storage_access_tracker.accessed_blocks.iter().map(|block_number| {
                    (BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number.0))
                }),
            );
            modified_contracts.extend(call_info.get_visited_contract_addresses());
            if let Some(class_hash) = call_info.call.class_hash {
                compiled_class_hash_keys.insert(class_hash);
            }
        }
    }

    storage_keys.extend(state_diff.storage.keys().copied());
    // Add the block hash table entries for the proof facts.
    for block_number in proof_facts_block_numbers {
        storage_keys.insert((BLOCK_HASH_TABLE_ADDRESS, StorageKey::from(block_number.0)));
    }
    // Add the alias contract entries if predictions are enabled.
    if include_alias_predictions {
        storage_keys.extend(predicted_alias_storage_entries(state_diff, alias_contract_address));
    }

    modified_contracts.extend(storage_keys.iter().map(|(address, _)| *address));
    modified_contracts.extend(state_diff.get_contract_addresses());

    compiled_class_hash_keys.extend(state_diff.class_hashes.values().copied());
    compiled_class_hash_keys.extend(state_diff.compiled_class_hashes.keys().copied());

    AccessedKeys { storage_keys, modified_contracts, compiled_class_hash_keys }
}
