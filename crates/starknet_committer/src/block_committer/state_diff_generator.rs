use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::Rng;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{StarknetStorageKey, StarknetStorageValue, StateDiff};

#[cfg(test)]
#[path = "state_diff_generator_test.rs"]
pub mod state_diff_generator_test;

pub(crate) const GENERATED_CONTRACT_ADDRESS: u32 = 500_u32;
pub(crate) const N_STORAGE_UPDATES: usize = 1000_usize;

pub fn generate_random_state_diff(rng: &mut StdRng) -> StateDiff {
    let mut storage_updates = HashMap::new();
    let mut contract_updates = HashMap::new();
    for _ in 0..N_STORAGE_UPDATES {
        let key = StarknetStorageKey(StorageKey(PatriciaKey::from_hex_unchecked(
            &generate_random_hex_251_bits(rng),
        )));
        let value =
            StarknetStorageValue(Felt::from_hex_unchecked(&generate_random_hex_251_bits(rng)));
        contract_updates.insert(key, value);
    }

    storage_updates.insert(ContractAddress::from(GENERATED_CONTRACT_ADDRESS), contract_updates);
    StateDiff {
        address_to_class_hash: Default::default(),
        address_to_nonce: Default::default(),
        class_hash_to_compiled_class_hash: HashMap::new(),
        storage_updates,
    }
}

fn generate_random_hex_251_bits(rng: &mut StdRng) -> String {
    const N_BYTES: usize = 32; // 251 bits / 8 bits per byte
    let mut random_bytes = [0u8; N_BYTES];
    rng.fill(&mut random_bytes);

    // Mask the extra bits if n_bits is not a multiple of 8
    let n_bits = 251;
    let mask = (1u8 << (n_bits % 8)) - 1;
    random_bytes[0] &= mask;

    hex::encode(random_bytes)
}
