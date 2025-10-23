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

pub fn generate_random_state_diff<R: Rng>(rng: &mut R, n_storage_updates: usize) -> StateDiff {
    let mut storage_updates = HashMap::new();
    let mut contract_updates = HashMap::with_capacity(n_storage_updates);
    for _ in 0..n_storage_updates {
        let storage_entry = generate_random_storage_entry(rng);
        contract_updates.insert(storage_entry.0, storage_entry.1);
    }

    storage_updates
        .insert(ContractAddress::from(RANDOM_STATE_DIFF_CONTRACT_ADDRESS), contract_updates);
    StateDiff { storage_updates, ..Default::default() }
}

fn generate_random_storage_entry<R: Rng>(
    rng: &mut R,
) -> (StarknetStorageKey, StarknetStorageValue) {
    let key = StarknetStorageKey(StorageKey(PatriciaKey::random(rng, None)));
    let value = StarknetStorageValue::random(rng, None);
    (key, value)
}
