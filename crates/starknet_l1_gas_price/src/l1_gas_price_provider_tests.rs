use starknet_api::block::{BlockNumber, BlockTimestamp};

use crate::l1_gas_price_provider::{L1GasPriceProvider, LAG_MARGIN_SECONDS, MEAN_NUMBER_OF_BLOCKS};

#[tokio::test]
async fn gas_price_provider_mean_prices() {
    let data_gas_price_const = 42;
    let num_added_blocks = 10;
    let mut provider = L1GasPriceProvider::default();
    let num_blocks = MEAN_NUMBER_OF_BLOCKS * 2;
    let final_timestamp = num_blocks * 2;
    for i in 0..=num_blocks {
        provider
            .add_price_info(BlockNumber(i), BlockTimestamp(i * 2), i.into(), data_gas_price_const)
            .unwrap();
    }

    // This calculation will grap all data from final_timestamp - MEAN_NUMBER_OF_BLOCKS to
    // final_timestamp.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + LAG_MARGIN_SECONDS)).unwrap();
    // The gas prices (set arbitrarily to equal the block number) should go from 300 to 600 in this
    // range. So the mean is 450.
    let gas_price_calculation =
        (MEAN_NUMBER_OF_BLOCKS + 1..=num_blocks).sum::<u64>() / MEAN_NUMBER_OF_BLOCKS;
    // The data gas price is set to a const, so this should be reflected in the result.
    assert_eq!(gas_price, gas_price_calculation.into());
    assert_eq!(data_gas_price, data_gas_price_const);

    // Add a few more blocks to the provider.
    for i in 1..num_added_blocks {
        provider
            .add_price_info(
                BlockNumber(num_blocks + i),
                BlockTimestamp(final_timestamp + i * 2),
                (num_blocks + i).into(),
                data_gas_price_const,
            )
            .unwrap();
    }

    // This should not change the results if we as for the same timestamp.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + LAG_MARGIN_SECONDS)).unwrap();
    assert_eq!(gas_price, gas_price_calculation.into());
    assert_eq!(data_gas_price, data_gas_price_const);

    // But if we take a slightly higher timestamp the gas price should go up.
    // Data gas price remains constant.
    let (gas_price, data_gas_price) =
        provider.get_price_info(BlockTimestamp(final_timestamp + LAG_MARGIN_SECONDS + 5)).unwrap();
    assert!(gas_price > gas_price_calculation.into());
    assert_eq!(data_gas_price, data_gas_price_const);

    // If we add a very high data gas price sample, it should increase the mean data gas price.
    provider
        .add_price_info(
            BlockNumber(num_blocks + num_added_blocks),
            BlockTimestamp(final_timestamp + num_added_blocks * 2),
            (num_blocks + num_added_blocks).into(),
            data_gas_price_const * 100,
        )
        .unwrap();

    let (_, data_gas_price) = provider
        .get_price_info(BlockTimestamp(final_timestamp + num_added_blocks * 2 + LAG_MARGIN_SECONDS))
        .unwrap();
    assert!(data_gas_price > data_gas_price_const);
}
