#![allow(dead_code)]
use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::transaction_execution::Transaction;
use starknet_api::contract_class::ContractClass;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;

fn diff_to_maps(diff: StateDiff) -> StateMaps {
    StateMaps {
        storage: diff
            .storage_updates
            .into_iter()
            .flat_map(|(address, updates)| {
                updates.into_iter().map(move |(key, value)| ((address, key.0), value.0))
            })
            .collect(),
        nonces: diff.address_to_nonce,
        class_hashes: diff.address_to_class_hash,
        compiled_class_hashes: diff
            .class_hash_to_compiled_class_hash
            .into_iter()
            .map(|(k, v)| (k, starknet_api::core::CompiledClassHash(v.0)))
            .collect(),
        ..Default::default()
    }
}

fn maps_to_diff(maps: StateMaps) -> StateDiff {
    StateDiff {
        storage_updates: maps.storage.into_iter().fold(
            HashMap::new(),
            |mut acc, ((address, key), value)| {
                acc.entry(address)
                    .or_insert_with(HashMap::new)
                    .insert(StarknetStorageKey(key), StarknetStorageValue(value));
                acc
            },
        ),
        address_to_nonce: maps.nonces,
        address_to_class_hash: maps.class_hashes,
        class_hash_to_compiled_class_hash: maps
            .compiled_class_hashes
            .into_iter()
            .map(|(k, v)| (k, CompiledClassHash(v.0)))
            .collect(),
    }
}

pub(crate) trait FlowTestState: Clone + UpdatableState + Sync + Send + 'static {
    fn create_empty_state() -> Self;

    /// Hook to preprocess the state before executing the transactions.
    fn preprocess_before_execution(&mut self, txs: &[Transaction]);

    /// Given a state diff with possible trivial entries (e.g., storage updates that set a value to
    /// it's previous value), return a state diff with only non-trivial entries.
    fn nontrivial_diff(&self, mut diff: StateDiff) -> StateDiff {
        // Remove trivial storage updates.
        diff.storage_updates.retain(|address, updates| {
            updates.retain(|key, value| {
                let current_value = self.get_storage_at(*address, key.0).unwrap_or_default();
                &current_value != &value.0
            });
            !updates.is_empty()
        });

        // Remove trivial nonce updates.
        diff.address_to_nonce.retain(|address, new_nonce| {
            let current_nonce = self.get_nonce_at(*address).unwrap_or_default();
            &current_nonce != new_nonce
        });

        // Remove trivial class hash updates.
        diff.address_to_class_hash.retain(|address, new_class_hash| {
            let current_class_hash = self.get_class_hash_at(*address).unwrap_or_default();
            &current_class_hash != new_class_hash
        });

        diff.class_hash_to_compiled_class_hash.retain(|class_hash, compiled_hash| {
            // Assume V2 hashes are stored in the state.
            let current_compiled_class_hash =
                self.get_compiled_class_hash_v2(*class_hash).unwrap_or_default();
            current_compiled_class_hash.0 != compiled_hash.0
        });

        diff
    }

    /// Same as [Self::nontrivial_diff] but works on [StateMaps].
    fn nontrivial_diff_maps(&self, maps: StateMaps) -> StateMaps {
        diff_to_maps(self.nontrivial_diff(maps_to_diff(maps)))
    }
}

impl FlowTestState for DictStateReader {
    fn create_empty_state() -> Self {
        DictStateReader::default()
    }

    /// Find all Cairo1 declares and explicitly add the compiled class hashes to the state, assuming
    /// they are blake (V2) hashes.
    /// When this trait is implemented on the [DictStateReader], it is required to store the V2
    /// hashes before executing the transactions, to indicate that migration is NOT required.
    fn preprocess_before_execution(&mut self, txs: &[Transaction]) {
        for tx in txs.iter() {
            if let Transaction::Account(account_tx) = tx {
                if let AccountTransaction::Declare(ref declare_tx) = account_tx.tx {
                    if let ContractClass::V1(_) = declare_tx.class_info.contract_class {
                        let class_hash = declare_tx.class_hash();
                        let compiled_class_hash = declare_tx.compiled_class_hash();
                        self.class_hash_to_compiled_class_hash_v2
                            .insert(class_hash, compiled_class_hash);
                    }
                }
            }
        }
    }
}
