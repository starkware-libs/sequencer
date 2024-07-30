use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::felt;
use starknet_api::state::StorageKey;

use crate::abi::constants;
use crate::blockifier::block::{
    pre_process_block,
    BlockInfo,
    BlockNumberHashPair,
    GAS_PRICE_MAX_CHANGE_DENOMINATOR,
    MAX_BLOCK_SIZE,
};
use crate::context::ChainInfo;
use crate::state::state_api::StateReader;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{CairoVersion, BALANCE};

#[test]
fn test_pre_process_block() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);
    let mut state = test_state(&ChainInfo::create_for_testing(), BALANCE, &[(test_contract, 1)]);

    // Test the positive flow of pre_process_block inside the allowed block number interval
    let block_number = BlockNumber(constants::STORED_BLOCK_HASH_BUFFER);
    let block_hash = felt!(20_u8);
    pre_process_block(
        &mut state,
        Some(BlockNumberHashPair::new(block_number.0, block_hash)),
        block_number,
    )
    .unwrap();

    let written_hash = state.get_storage_at(
        ContractAddress::from(constants::BLOCK_HASH_CONTRACT_ADDRESS),
        StorageKey::from(block_number.0),
    );
    assert_eq!(written_hash.unwrap(), block_hash);

    // Test that block pre-process with block hash None is successful only within the allowed
    // block number interval.
    let block_number = BlockNumber(constants::STORED_BLOCK_HASH_BUFFER - 1);
    assert!(pre_process_block(&mut state, None, block_number).is_ok());

    let block_number = BlockNumber(constants::STORED_BLOCK_HASH_BUFFER);
    let error = pre_process_block(&mut state, None, block_number);
    assert_eq!(
        format!(
            "A block hash must be provided for block number > {}.",
            constants::STORED_BLOCK_HASH_BUFFER
        ),
        format!("{}", error.unwrap_err())
    );
}

fn compute_and_check_price_range(
    prev_price: u64,
    gas_used: u64,
    gas_target: u64,
    should_increase: bool,
) -> u64 {
    let new_price = BlockInfo::calculate_next_base_gas_price(prev_price, gas_used, gas_target);

    let prev_price_f64 = f64::from(u32::try_from(prev_price).unwrap());
    let new_price_f64 = f64::from(u32::try_from(new_price).unwrap());

    let ratio = new_price_f64 / prev_price_f64;
    let max_change = 1.0 / f64::from(u32::try_from(GAS_PRICE_MAX_CHANGE_DENOMINATOR).unwrap());
    if should_increase {
        assert!((1.0..=1.0 + max_change).contains(&ratio));
    } else {
        assert!((1.0 - max_change..=1.0).contains(&ratio));
    }
    new_price
}

#[test]
fn test_congestion_increases_price() {
    let mut prev_price = 1000000;
    let current_gas_target = MAX_BLOCK_SIZE / 2;
    let gas_usages = [MAX_BLOCK_SIZE * 2 / 3, MAX_BLOCK_SIZE * 3 / 4];

    for gas_used in gas_usages {
        prev_price = compute_and_check_price_range(prev_price, gas_used, current_gas_target, true);
    }
}

#[test]
fn test_reduced_gas_usage_decreases_price() {
    let mut prev_price = 1000000;
    let current_gas_target = MAX_BLOCK_SIZE / 2;
    let gas_usages = [MAX_BLOCK_SIZE * 3 / 8, MAX_BLOCK_SIZE * 1 / 3];

    for gas_used in gas_usages {
        prev_price = compute_and_check_price_range(prev_price, gas_used, current_gas_target, false);
    }
}

#[test]
fn test_stable_gas_usage() {
    let mut current_price = 1000000;
    let mut prev_price = 1000000;
    let current_gas_target = MAX_BLOCK_SIZE / 2;
    let gas_usages = [MAX_BLOCK_SIZE / 2, MAX_BLOCK_SIZE / 2];

    for &gas_used in &gas_usages {
        current_price =
            BlockInfo::calculate_next_base_gas_price(current_price, gas_used, current_gas_target);
        assert_eq!(current_price, prev_price);
        prev_price = current_price;
    }
}

#[test]
// This test ensures that the gas price calculation does not overflow with extreme values,
// verifying that `calculate_next_base_gas_price` does not panic.
fn test_gas_price_with_extreme_values() {
    let price = u64::MAX;
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = 0;
    BlockInfo::calculate_next_base_gas_price(price, gas_used, gas_target);

    // To avoid overflow when updating the price, the value is set below a certain threshold so that
    // the new price does not exceed u64::MAX.
    let price = u64::MAX - (u64::MAX / u64::try_from(GAS_PRICE_MAX_CHANGE_DENOMINATOR).unwrap());
    let gas_target = MAX_BLOCK_SIZE / 2;
    let gas_used = MAX_BLOCK_SIZE;
    BlockInfo::calculate_next_base_gas_price(price, gas_used, gas_target);
}
