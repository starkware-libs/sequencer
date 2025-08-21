use std::collections::HashMap;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rstest::{fixture, rstest};
use starknet_api::core::ContractAddress;

use crate::block_committer::state_diff_generator::{
    generate_random_state_diff,
    generate_random_storage_entry,
    N_STORAGE_UPDATES,
    RANDOM_STATE_DIFF_CONTRACT_ADDRESS,
};

#[fixture]
fn rng() -> SmallRng {
    let seed = 42_u64; // Constant seed for reproducibility.
    SmallRng::seed_from_u64(seed)
}

#[rstest]
fn generate_random_state_diff_test(mut rng: impl Rng) {
    let state_diff = generate_random_state_diff(&mut rng);
    let contract = state_diff
        .storage_updates
        .get(&ContractAddress::from(RANDOM_STATE_DIFF_CONTRACT_ADDRESS))
        .unwrap();
    assert_eq!(contract.len(), N_STORAGE_UPDATES);
}

#[rstest]
fn key_distribution_test(mut rng: impl Rng) {
    let n_iterations = N_STORAGE_UPDATES * 100;
    let mut storage_updates = HashMap::with_capacity(n_iterations);
    for _ in 0..n_iterations {
        let (key, value) = generate_random_storage_entry(&mut rng);
        storage_updates.insert(key, value);
    }
    assert!(storage_updates.len() >= (n_iterations * 99 / 100), "Key distribution is limited");
}
