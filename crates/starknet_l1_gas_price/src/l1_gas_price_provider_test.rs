use papyrus_base_layer::PriceSample;
use starknet_api::block::{BlockNumber, BlockTimestamp};

use crate::l1_gas_price_provider::{L1GasPriceProvider, L1GasPriceProviderConfig};

// Make a provider with five samples.
// Timestamps are 2 seconds per block, from zero to 8.
// To average over the middle 3 samples, we should use timestamps 2, 4, and 6.
// To get the timestamp at the end of the range, we need to ask for timestamp
// of 6 + 60 (which is the lag margin).
// Returns the provider, the final timestamp, and the lag margin.
fn make_provider_with_few_samples() -> (L1GasPriceProvider, u64, u64) {
    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: 3,
        ..Default::default()
    });

    for i in 0..5 {
        let block_num = i.try_into().unwrap();
        let price = i.try_into().unwrap();
        let time = (i * 2).try_into().unwrap();
        let sample = PriceSample { timestamp: time, base_fee_per_gas: price, blob_fee: price + 1 };
        provider.add_price_info(BlockNumber(block_num), sample).unwrap();
    }
    let lag = provider.config.lag_margin_seconds;
    let final_timestamp = 6;
    (provider, final_timestamp, lag)
}

#[test]
fn gas_price_provider_mean_prices() {
    let (provider, final_timestamp, lag) = make_provider_with_few_samples();

    // This calculation will grab 3 samples from the middle of the range.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();
    // The gas prices (set arbitrarily to equal the block number) should go from
    let gas_price_calculation = (1 + 2 + 3) / 3;
    // The data gas is one more than the gas price.
    let data_gas_calculation = gas_price_calculation + 1;
    assert_eq!(gas_price, gas_price_calculation);
    assert_eq!(data_gas_price, data_gas_calculation);
}

#[test]
fn gas_price_provider_adding_samples() {
    let (mut provider, final_timestamp, lag) = make_provider_with_few_samples();

    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // Add a block to the provider.
    let sample = PriceSample { timestamp: 10, base_fee_per_gas: 10, blob_fee: 11 };
    provider.add_price_info(BlockNumber(5), sample).unwrap();

    let (gas_price_new, data_gas_price_new) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // This should not change the results if we ask for the same timestamp.
    assert_eq!(gas_price, gas_price_new);
    assert_eq!(data_gas_price, data_gas_price_new);
}

#[test]
fn gas_price_provider_timestamp_changes_mean() {
    let (provider, final_timestamp, lag) = make_provider_with_few_samples();

    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // If we take a higher timestamp the gas prices should go up.
    let (gas_price_new, data_gas_price_new) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag + 2)).unwrap();
    assert!(gas_price_new > gas_price);
    assert!(data_gas_price_new > data_gas_price);
}


fn foo() -> u64 { 20 }