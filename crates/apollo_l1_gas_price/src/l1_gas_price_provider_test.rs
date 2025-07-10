use apollo_l1_gas_price_types::{GasPriceData, PriceInfo};
use starknet_api::block::{BlockTimestamp, GasPrice};

use crate::l1_gas_price_provider::{
    L1GasPriceProvider,
    L1GasPriceProviderConfig,
    L1GasPriceProviderError,
};

// Make a provider with five block prices. Timestamps are 2 seconds apart, starting from 0.
// To get the prices for the middle three blocks use the timestamp for block[3].
// Returns the provider, a vector of block prices to compare with, and the timestamp of block[3].
fn make_provider() -> (L1GasPriceProvider, Vec<PriceInfo>, u64) {
    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: 3,
        ..Default::default()
    });
    provider.initialize().unwrap();
    let mut prices = Vec::new();
    let mut timestamp3 = 0;
    for i in 0..5 {
        let block_number = i.try_into().unwrap();
        let price = (i * i).try_into().unwrap();
        let time = (i * 2).try_into().unwrap();
        let price_info =
            PriceInfo { base_fee_per_gas: GasPrice(price), blob_fee: GasPrice(price + 1) };
        prices.push(price_info.clone());
        if i == 3 {
            timestamp3 = time;
        }
        provider
            .add_price_info(GasPriceData {
                block_number,
                timestamp: BlockTimestamp(time),
                price_info,
            })
            .unwrap();
    }
    (provider, prices, timestamp3)
}

#[test]
fn gas_price_provider_mean_prices() {
    let (provider, block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds;
    let num_blocks: u128 = provider.config.number_of_blocks_for_mean.into();

    // This calculation will grab config.number_of_blocks_for_mean prices from the middle of the
    // range. timestamp3 (for block_prices[3]) is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // The gas prices should go from block 1 to 3.
    let gas_price_calculation = block_prices[1]
        .base_fee_per_gas
        .saturating_add(block_prices[2].base_fee_per_gas)
        .saturating_add(block_prices[3].base_fee_per_gas)
        .checked_div(num_blocks)
        .expect("Cannot divide by zero");
    let data_price_calculation = block_prices[1]
        .blob_fee
        .saturating_add(block_prices[2].blob_fee)
        .saturating_add(block_prices[3].blob_fee)
        .checked_div(num_blocks)
        .expect("Cannot divide by zero");
    assert_eq!(gas_price, gas_price_calculation);
    assert_eq!(data_gas_price, data_price_calculation);
}

#[test]
fn gas_price_provider_adding_blocks() {
    let (mut provider, _block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds;

    // timestamp3 is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // Add a block to the provider.
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(10), blob_fee: GasPrice(11) };
    let timestamp = BlockTimestamp(10);
    provider.add_price_info(GasPriceData { block_number: 5, timestamp, price_info }).unwrap();

    // This should not change the results if we ask for the same timestamp.
    let PriceInfo { base_fee_per_gas: gas_price_new, blob_fee: data_gas_price_new } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    assert_eq!(gas_price, gas_price_new);
    assert_eq!(data_gas_price, data_gas_price_new);

    // Add another block to the provider.
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(12), blob_fee: GasPrice(13) };
    let timestamp = BlockTimestamp(12);
    provider.add_price_info(GasPriceData { block_number: 6, timestamp, price_info }).unwrap();

    // Should fail because the memory of the provider is full, and we added another block.
    let ret = provider.get_price_info(BlockTimestamp(timestamp3 + lag));
    matches!(ret, Result::Err(L1GasPriceProviderError::MissingDataError { .. }));
}

#[test]
fn gas_price_provider_timestamp_changes_mean() {
    let (provider, _block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds;

    // timestamp3 is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // If we take a higher timestamp the gas prices should change.
    let PriceInfo { base_fee_per_gas: gas_price_new, blob_fee: data_gas_price_new } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag * 2)).unwrap();
    assert_ne!(gas_price_new, gas_price);
    assert_ne!(data_gas_price_new, data_gas_price);
}

#[test]
fn gas_price_provider_can_start_at_nonzero_height() {
    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: 3,
        ..Default::default()
    });
    provider.initialize().unwrap();
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(0), blob_fee: GasPrice(0) };
    let timestamp = BlockTimestamp(0);
    provider.add_price_info(GasPriceData { block_number: 42, timestamp, price_info }).unwrap();
}

#[test]
fn gas_price_provider_uninitialized_error() {
    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: 3,
        ..Default::default()
    });
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(0), blob_fee: GasPrice(0) };
    let timestamp = BlockTimestamp(0);
    let result = provider.add_price_info(GasPriceData { block_number: 42, timestamp, price_info });
    assert!(matches!(result, Err(L1GasPriceProviderError::NotInitializedError)));
}

#[test]
fn gas_price_provider_sanity_check() {
    const NUM_SAMPLES: usize = 10;
    const NUM_ROUNDS: usize = 30;
    const EXTRA_SAMPLES: usize = 5; // Extra samples to ensure the provider can handle more than NUM_SAMPLES.
    const ETH_BLOCK_TIME: u64 = 12; // seconds
    const LAG_TIME: u64 = 60; // seconds

    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: NUM_SAMPLES.try_into().unwrap(),
        storage_limit: 10 * NUM_SAMPLES,
        lag_margin_seconds: LAG_TIME,
        ..Default::default()
    });
    provider.initialize().unwrap();
    for i in 0..NUM_SAMPLES * NUM_ROUNDS + EXTRA_SAMPLES {
        let block_number = 1000 + u64::try_from(i).unwrap();
        // Around 10M gas price, with a small variation.
        let gas_price = 10000000 + 1000000 * u128::try_from(i % 3).unwrap();
        // Around 35 data price, with a small variation.
        let data_price = 35 + u128::try_from(i % 6).unwrap();
        let time = u64::try_from(i).unwrap() * ETH_BLOCK_TIME;
        let price_info =
            PriceInfo { base_fee_per_gas: GasPrice(gas_price), blob_fee: GasPrice(data_price) };
        provider
            .add_price_info(GasPriceData {
                block_number,
                timestamp: BlockTimestamp(time),
                price_info,
            })
            .unwrap();
    }
    println!("Added {} samples to the provider", NUM_SAMPLES);
    // println!("Provider state: {:#?}", provider);

    let timestamp = BlockTimestamp(
        ETH_BLOCK_TIME * u64::try_from(NUM_SAMPLES * NUM_ROUNDS + EXTRA_SAMPLES).unwrap()
            + LAG_TIME,
    );
    println!(
        "\n\n The average gas price is {} and data gas price: {}",
        provider.get_price_info(timestamp).unwrap().base_fee_per_gas,
        provider.get_price_info(timestamp).unwrap().blob_fee
    );
}
