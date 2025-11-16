use std::collections::HashMap;

use rand::Rng;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;

use crate::block_committer::input::{StarknetStorageKey, StarknetStorageValue, StateDiff};
use crate::block_committer::random_structs::RandomValue;

#[cfg(test)]
#[path = "state_diff_generator_test.rs"]
pub mod state_diff_generator_test;

pub const RANDOM_STATE_DIFF_CONTRACT_ADDRESS: u32 = 500_u32;

/// Generates a random state diff with the given number of storage updates.
/// If `keys_override` is provided, use it as the keys for the storage updates.
/// Otherwise, generates random keys.
pub fn generate_random_state_diff<R: Rng>(
    rng: &mut R,
    n_storage_updates: usize,
    keys_override: Option<Vec<StarknetStorageKey>>,
) -> StateDiff {
    if let Some(keys_override) = keys_override.as_ref() {
        assert_eq!(
            keys_override.len(),
            n_storage_updates,
            "Number of keys override must match number of storage updates"
        );
    }
    let mut storage_updates = HashMap::new();
    let mut contract_updates = HashMap::with_capacity(n_storage_updates);
    for i in 0..n_storage_updates {
        let storage_entry =
            generate_random_storage_entry(rng, keys_override.as_ref().map(|v| v[i]));
        contract_updates.insert(storage_entry.0, storage_entry.1);
    }

    storage_updates
        .insert(ContractAddress::from(RANDOM_STATE_DIFF_CONTRACT_ADDRESS), contract_updates);
    StateDiff { storage_updates, ..Default::default() }
}

fn generate_random_storage_entry<R: Rng>(
    rng: &mut R,
    key_override: Option<StarknetStorageKey>,
) -> (StarknetStorageKey, StarknetStorageValue) {
    let key =
        key_override.unwrap_or(StarknetStorageKey(StorageKey(PatriciaKey::random(rng, None))));
    let value = StarknetStorageValue::random(rng, None);
    (key, value)
}
