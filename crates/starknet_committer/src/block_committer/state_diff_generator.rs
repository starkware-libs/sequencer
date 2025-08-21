use std::collections::HashMap;

use rand::rngs::StdRng;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;

use crate::block_committer::input::{StarknetStorageKey, StarknetStorageValue, StateDiff};
use crate::block_committer::random_structs::RandomValue;

#[cfg(test)]
#[path = "state_diff_generator_test.rs"]
pub mod state_diff_generator_test;

pub(crate) const CONTRACT_ADDRESS: u32 = 500_u32;
pub(crate) const N_STORAGE_UPDATES: usize = 1000_usize;

pub fn generate_random_state_diff(rng: &mut StdRng) -> StateDiff {
    let mut storage_updates = HashMap::new();
    let mut contract_updates = HashMap::with_capacity(N_STORAGE_UPDATES);
    for _ in 0..N_STORAGE_UPDATES {
        let storage_entry = generate_random_storage_entry(rng);
        contract_updates.insert(storage_entry.0, storage_entry.1);
    }

    storage_updates.insert(ContractAddress::from(CONTRACT_ADDRESS), contract_updates);
    StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: HashMap::new(),
        storage_updates,
    }
}

pub(crate) fn generate_random_storage_entry(
    rng: &mut StdRng,
) -> (StarknetStorageKey, StarknetStorageValue) {
    let key = StarknetStorageKey(StorageKey(PatriciaKey::random(rng, None)));
    let value = StarknetStorageValue::random(rng, None);
    (key, value)
}
