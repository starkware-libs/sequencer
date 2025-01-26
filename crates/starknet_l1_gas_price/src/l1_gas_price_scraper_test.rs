use std::sync::Arc;

use papyrus_base_layer::{MockBaseLayerContract, PriceSample};

use crate::l1_gas_price_provider::MockL1GasPriceProviderClient;
use crate::l1_gas_price_scraper::{L1GasPriceScraper, L1GasPriceScraperConfig};

const BLOCK_TIME: u64 = 2;
const GAS_PRICE: u128 = 42;
const DATA_PRICE: u128 = 137;

#[allow(clippy::as_conversions)]
fn setup_scraper(
    end_block: u64,
    expected_number_of_blocks: usize,
) -> L1GasPriceScraper<MockBaseLayerContract> {
    let mut mock_contract = MockBaseLayerContract::new();
    mock_contract.expect_get_price_sample().returning(move |number| {
        if number >= end_block {
            Ok(None)
        } else {
            Ok(Some(PriceSample {
                timestamp: number * BLOCK_TIME,
                base_fee_per_gas: number as u128 * GAS_PRICE,
                blob_fee: number as u128 * DATA_PRICE,
            }))
        }
    });

    let mut mock_provider = MockL1GasPriceProviderClient::new();
    mock_provider
        .expect_add_price_info()
        .withf(|number, price_sample| {
            price_sample.timestamp == number.0 * BLOCK_TIME
                && price_sample.base_fee_per_gas == number.0 as u128 * GAS_PRICE
                && price_sample.blob_fee == number.0 as u128 * DATA_PRICE
        })
        .times(expected_number_of_blocks)
        .returning(|_, _| Ok(()));

    L1GasPriceScraper::new(
        L1GasPriceScraperConfig::default(),
        Arc::new(mock_provider),
        mock_contract,
    )
}

#[tokio::test]
async fn run_l1_gas_price_scraper_single_block() {
    const START_BLOCK: u64 = 0;
    const END_BLOCK: u64 = 1;
    const EXPECT_NUMBER: usize = 1;
    let mut scraper = setup_scraper(END_BLOCK, EXPECT_NUMBER);
    scraper.update_prices(START_BLOCK).await.unwrap();
}

#[tokio::test]
#[allow(clippy::as_conversions)]
async fn run_l1_gas_price_scraper_multiple_blocks() {
    const START_BLOCK: u64 = 5;
    const END_BLOCK: u64 = 10;
    const EXPECT_NUMBER: usize = 5;
    let mut scraper = setup_scraper(END_BLOCK, EXPECT_NUMBER);

    // Should update prices from 5 to 10 (not inclusive) and on 10 get a None from base layer.
    scraper.update_prices(START_BLOCK).await.unwrap();
}
