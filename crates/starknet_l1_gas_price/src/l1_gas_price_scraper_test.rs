use std::sync::Arc;

use papyrus_base_layer::{MockBaseLayerContract, PriceSample};
use starknet_l1_gas_price_types::MockL1GasPriceProviderClient;

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
    mock_contract.expect_get_price_sample().returning(move |block_number| {
        if block_number >= end_block {
            Ok(None)
        } else {
            Ok(Some(PriceSample {
                timestamp: block_number * BLOCK_TIME,
                base_fee_per_gas: block_number as u128 * GAS_PRICE,
                blob_fee: block_number as u128 * DATA_PRICE,
            }))
        }
    });

    let mut mock_provider = MockL1GasPriceProviderClient::new();
    mock_provider
        .expect_add_price_info()
        .withf(|block_number, price_sample| {
            price_sample.timestamp == *block_number * BLOCK_TIME
                && price_sample.base_fee_per_gas == *block_number as u128 * GAS_PRICE
                && price_sample.blob_fee == *block_number as u128 * DATA_PRICE
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

#[allow(clippy::as_conversions)]
#[tokio::test]
async fn run_l1_gas_price_scraper_two_blocks() {
    const START_BLOCK: u64 = 2;
    const END_BLOCK1: u64 = 7;
    const END_BLOCK2: u64 = 12;

    // Explicitly making the mocks here, so we can customize them for the test.
    let mut mock_contract = MockBaseLayerContract::new();
    // Note the order of the expectation is important! Can only scrape the first blocks first.
    mock_contract
        .expect_get_price_sample()
        .times(END_BLOCK1 as usize - START_BLOCK as usize + 1)
        .returning(move |block_number| {
            if block_number >= END_BLOCK1 {
                Ok(None)
            } else {
                Ok(Some(PriceSample {
                    timestamp: block_number * BLOCK_TIME,
                    base_fee_per_gas: block_number as u128 * GAS_PRICE,
                    blob_fee: block_number as u128 * DATA_PRICE,
                }))
            }
        });
    mock_contract
        .expect_get_price_sample()
        .times(END_BLOCK2 as usize - END_BLOCK1 as usize + 1)
        .returning(move |block_number| {
            if block_number >= END_BLOCK2 {
                Ok(None)
            } else {
                Ok(Some(PriceSample {
                    timestamp: block_number * BLOCK_TIME,
                    base_fee_per_gas: block_number as u128 * GAS_PRICE,
                    blob_fee: block_number as u128 * DATA_PRICE,
                }))
            }
        });

    let mut mock_provider = MockL1GasPriceProviderClient::new();
    mock_provider
        .expect_add_price_info()
        .withf(|block_number, price_sample| {
            price_sample.timestamp == *block_number * BLOCK_TIME
                && price_sample.base_fee_per_gas == *block_number as u128 * GAS_PRICE
                && price_sample.blob_fee == *block_number as u128 * DATA_PRICE
        })
        .times(END_BLOCK2 as usize - START_BLOCK as usize)
        .returning(|_, _| Ok(()));

    let mut scraper = L1GasPriceScraper::new(
        L1GasPriceScraperConfig::default(),
        Arc::new(mock_provider),
        mock_contract,
    );

    let block_number = scraper.update_prices(START_BLOCK).await.unwrap();
    assert_eq!(block_number, END_BLOCK1);
    let block_number = scraper.update_prices(block_number).await.unwrap();
    assert_eq!(block_number, END_BLOCK2);
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
