use std::collections::HashMap;
use tempfile::tempdir;

use rand::rngs::StdRng;
use rand::SeedableRng;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue, Storage};

use crate::block_committer::state_diff_generator::{
    generate_random_state_diff,
    generate_random_storage_entry,
    CONTRACT_ADDRESS,
    N_STORAGE_UPDATES,
};

#[rstest]
fn generate_random_state_diff_test() {
    let seed = 42_u64; // Constant seed for reproducibility.
    let mut rng = StdRng::seed_from_u64(seed);
    let state_diff = generate_random_state_diff(&mut rng);
    let contract =
        state_diff.storage_updates.get(&ContractAddress::from(CONTRACT_ADDRESS)).unwrap();
    assert_eq!(contract.len(), N_STORAGE_UPDATES);
}

#[rstest]
fn key_distribution_test() {
    let seed = 42_u64; // Constant seed for reproducibility.
    let mut rng = StdRng::seed_from_u64(seed);

    let n_iterations = N_STORAGE_UPDATES * 100;

    let mut storage_updates = HashMap::with_capacity(n_iterations);
    let mut mdbx_storage = MdbxStorage::open(tempdir().unwrap().path()).unwrap();

    for _ in 0..n_iterations {
        let (key, value) = generate_random_storage_entry(&mut rng);
        storage_updates.insert(key, value);
    }

    mdbx_storage
        .mset(
            storage_updates
                .iter()
                .map(|(key, value)| (DbKey::from(key), DbValue::from(value)))
                .collect(),
        )
        .unwrap();

    for (key, value) in &storage_updates {
        let stored_value = mdbx_storage.get(&DbKey::from(key)).unwrap().unwrap();
        assert_eq!(stored_value, DbValue::from(value));
    }

    assert!(storage_updates.len() >= (n_iterations * 99 / 100), "Key distribution is limited");
}
