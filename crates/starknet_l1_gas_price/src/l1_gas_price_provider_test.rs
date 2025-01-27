use papyrus_base_layer::PriceSample;
use starknet_api::block::{BlockNumber, BlockTimestamp};

use crate::l1_gas_price_provider::{
    L1GasPriceProvider,
    L1GasPriceProviderConfig,
    L1GasPriceProviderError,
};

// Make a provider with five samples.
// Timestamps are 2 seconds apart, from zero to 8.
// To average over the middle 3 samples (number_of_blocks_for_mean), we should use timestamps
// 2, 4, and 6. To get the timestamp at the end of the range, we need to ask for timestamp
// of 6 + 60 (which is the lag margin from the config).
// Returns the provider and a vector of samples to compare with.
fn make_provider() -> (L1GasPriceProvider, Vec<PriceSample>) {
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
    (provider, samples)
}

#[test]
fn gas_price_provider_mean_prices() {
    let (provider, samples) = make_provider();
    let lag = provider.config.lag_margin_seconds;
    let number: u128 = provider.config.number_of_blocks_for_mean.into();
    // Timestamp for sample[3] is used to define the interval of samples 1 to 3.
    let final_timestamp = samples[3].timestamp;

    // This calculation will grab config.number_of_blocks_for_mean samples from the middle of the
    // range.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // The gas prices should go from block 1 to 3.
    let gas_price_calculation =
        (samples[1].base_fee_per_gas + samples[2].base_fee_per_gas + samples[3].base_fee_per_gas)
            / number;
    let data_price_calculation =
        (samples[1].blob_fee + samples[2].blob_fee + samples[3].blob_fee) / number;
    assert_eq!(gas_price, gas_price_calculation);
    assert_eq!(data_gas_price, data_price_calculation);
}

#[test]
fn gas_price_provider_adding_samples() {
    let (mut provider, samples) = make_provider();
    let lag = provider.config.lag_margin_seconds;
    // Timestamp for sample[3] is used to define the interval of samples 1 to 3.
    let final_timestamp = samples[3].timestamp;

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
    let (provider, samples) = make_provider();
    let lag = provider.config.lag_margin_seconds;
    // Timestamp for sample[3] is used to define the interval of samples 1 to 3.
    let final_timestamp = samples[3].timestamp;

    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag)).unwrap();

    // If we take a higher timestamp the gas prices should change.
    let (gas_price_new, data_gas_price_new) =
        provider.get_price_info(BlockTimestamp(final_timestamp + lag * 2)).unwrap();
    assert_ne!(gas_price_new, gas_price);
    assert_ne!(data_gas_price_new, data_gas_price);
}
