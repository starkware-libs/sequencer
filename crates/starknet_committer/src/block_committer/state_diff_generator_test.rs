use rand::rngs::StdRng;
use rand::SeedableRng;
use rstest::rstest;
use starknet_api::core::ContractAddress;

use crate::block_committer::state_diff_generator::{
    generate_random_state_diff,
    GENERATED_CONTRACT_ADDRESS,
    N_STORAGE_UPDATES,
};

#[rstest]
fn generate_random_state_diff_test() {
    let seed = 42_u64; // Constant seed for reproducibility
    let mut rng = StdRng::seed_from_u64(seed);
    let state_diff = generate_random_state_diff(&mut rng);
    let contract =
        state_diff.storage_updates.get(&ContractAddress::from(GENERATED_CONTRACT_ADDRESS)).unwrap();
    assert_eq!(contract.len(), N_STORAGE_UPDATES);
}
