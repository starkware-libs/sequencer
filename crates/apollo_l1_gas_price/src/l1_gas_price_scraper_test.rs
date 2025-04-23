use std::sync::Arc;

use apollo_l1_gas_price_types::MockL1GasPriceProviderClient;
use papyrus_base_layer::{L1BlockReference, MockBaseLayerContract, PriceSample};

use crate::l1_gas_price_scraper::{L1GasPriceScraper, L1GasPriceScraperConfig};

const BLOCK_TIME: u64 = 2;
const GAS_PRICE: u128 = 42;
const DATA_PRICE: u128 = 137;

fn setup_scraper(
    end_block: u64,
    expected_number_of_blocks: usize,
) -> L1GasPriceScraper<MockBaseLayerContract> {
    let mut mock_contract = MockBaseLayerContract::new();
    mock_contract.expect_get_price_sample_and_block_reference().returning(move |block_number| {
        if block_number >= end_block {
            Ok(None)
        } else {
            Ok(Some(price_sample_and_block_reference_from_block_number(block_number)))
        }
    });

    let mut mock_provider = MockL1GasPriceProviderClient::new();
    mock_provider
        .expect_add_price_info()
        .withf(|&block_number, price_sample| {
            price_sample.timestamp == block_number * BLOCK_TIME
                && price_sample.base_fee_per_gas == u128::from(block_number) * GAS_PRICE
                && price_sample.blob_fee == u128::from(block_number) * DATA_PRICE
        })
        .times(expected_number_of_blocks)
        .returning(|_, _| Ok(()));

    L1GasPriceScraper::new(
        L1GasPriceScraperConfig::default(),
        Arc::new(mock_provider),
        mock_contract,
    )
}

fn price_sample_and_block_reference_from_block_number(
    block_number: u64,
) -> (PriceSample, L1BlockReference) {
    let price_sample = PriceSample {
        timestamp: block_number * BLOCK_TIME,
        base_fee_per_gas: u128::from(block_number) * GAS_PRICE,
        blob_fee: u128::from(block_number) * DATA_PRICE,
    };
    let mut hash = [0u8; 32];
    let bytes = block_number.to_be_bytes();
    hash[24..].copy_from_slice(&bytes);
    let block_reference = L1BlockReference {
        number: block_number,
        // This is just a placeholder for real block hashes.
        hash,
    };
    (price_sample, block_reference)
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
async fn run_l1_gas_price_scraper_two_blocks() {
    const START_BLOCK: u64 = 2;
    const END_BLOCK1: u64 = 7;
    const END_BLOCK2: u64 = 12;

    // Explicitly making the mocks here, so we can customize them for the test.
    let mut mock_contract = MockBaseLayerContract::new();
    // Note the order of the expectation is important! Can only scrape the first blocks first.
    mock_contract
        .expect_get_price_sample_and_block_reference()
        .times(usize::try_from(END_BLOCK1 - START_BLOCK + 1).unwrap())
        .returning(move |block_number| {
            if block_number >= END_BLOCK1 {
                Ok(None)
            } else {
                Ok(Some(price_sample_and_block_reference_from_block_number(block_number)))
            }
        });
    mock_contract
        .expect_get_price_sample_and_block_reference()
        .times(usize::try_from(END_BLOCK2 - END_BLOCK1 + 1).unwrap())
        .returning(move |block_number| {
            if block_number >= END_BLOCK2 {
                Ok(None)
            } else {
                Ok(Some(price_sample_and_block_reference_from_block_number(block_number)))
            }
        });

    let mut mock_provider = MockL1GasPriceProviderClient::new();
    mock_provider
        .expect_add_price_info()
        .withf(|&block_number, price_sample| {
            price_sample.timestamp == block_number * BLOCK_TIME
                && price_sample.base_fee_per_gas == u128::from(block_number) * GAS_PRICE
                && price_sample.blob_fee == u128::from(block_number) * DATA_PRICE
        })
        .times(usize::try_from(END_BLOCK2 - START_BLOCK).unwrap())
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
async fn run_l1_gas_price_scraper_multiple_blocks() {
    const START_BLOCK: u64 = 5;
    const END_BLOCK: u64 = 10;
    const EXPECT_NUMBER: usize = 5;
    let mut scraper = setup_scraper(END_BLOCK, EXPECT_NUMBER);

    // Should update prices from 5 to 10 (not inclusive) and on 10 get a None from base layer.
    scraper.update_prices(START_BLOCK).await.unwrap();
}

// TODO(guyn): test scraper with a provider timeout
