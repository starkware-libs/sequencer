use papyrus_base_layer::PriceSample;
use starknet_api::block::{BlockNumber, BlockTimestamp};

use crate::l1_gas_price_provider::{
    L1GasPriceProvider,
    L1GasPriceProviderConfig,
    L1GasPriceProviderError,
};

// Make a provider with five samples.
// Timestamps are 2 seconds per block, from zero to 8.
// To average over the middle 3 samples, we should use timestamps 2, 4, and 6.
// To get the timestamp at the end of the range, we need to ask for timestamp
// of 6 + 60 (which is the lag margin).
// Returns the provider, the final timestamp, and the lag margin.
fn make_provider_with_few_samples() -> (L1GasPriceProvider, u64, Vec<PriceSample>) {
    let mut provider = L1GasPriceProvider::new(L1GasPriceProviderConfig {
        number_of_blocks_for_mean: 3,
        ..Default::default()
    });
    let mut samples = Vec::new();
    for i in 0..5 {
        let block_num = i.try_into().unwrap();
        let price = (i * i).try_into().unwrap();
        let time = (i * 2).try_into().unwrap();
        let sample = PriceSample { timestamp: time, base_fee_per_gas: price, blob_fee: price + 1 };
        samples.push(sample.clone());
        provider.add_price_info(BlockNumber(block_num), sample).unwrap();
    }
    let final_timestamp = 6;
    (provider, final_timestamp, samples)
}

#[test]
fn gas_price_provider_mean_prices() {
    let (provider, final_timestamp, samples) = make_provider_with_few_samples();
    let lag = provider.config.lag_margin_seconds;

    // This calculation will grab 3 samples from the middle of the range.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // The gas prices should go from block 1 to 3.
    let gas_price_calculation =
        (samples[1].base_fee_per_gas + samples[2].base_fee_per_gas + samples[3].base_fee_per_gas)
            / 3;
    let data_price_calculation =
        (samples[1].blob_fee + samples[2].blob_fee + samples[3].blob_fee) / 3;
    assert_eq!(gas_price, gas_price_calculation);
    assert_eq!(data_gas_price, data_price_calculation);
}

#[test]
fn gas_price_provider_adding_samples() {
    let (mut provider, final_timestamp, _) = make_provider_with_few_samples();
    let lag = provider.config.lag_margin_seconds;

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

    // Add another block to the provider.
    let sample = PriceSample { timestamp: 12, base_fee_per_gas: 12, blob_fee: 13 };
    provider.add_price_info(BlockNumber(6), sample).unwrap();

    // Should fail because the memory of the provider is full, and we added another block.
    let ret = provider.get_price_info(BlockTimestamp(final_timestamp + lag));
    matches!(ret, Result::Err(L1GasPriceProviderError::MissingData(_)));
}
#[test]
fn gas_price_provider_timestamp_changes_mean() {
    let (provider, final_timestamp, _) = make_provider_with_few_samples();
    let lag = provider.config.lag_margin_seconds;

    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // If we take a higher timestamp the gas prices should change.
    let (gas_price_new, data_gas_price_new) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag * 2)).unwrap();
    assert_ne!(gas_price_new, gas_price);
    assert_ne!(data_gas_price_new, data_gas_price);
}
