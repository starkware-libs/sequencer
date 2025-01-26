use core::num;
use std::fmt::{Display, Error};
use std::ops::RangeInclusive;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_base_layer::{
    BaseLayerContract,
    L1BlockNumber,
    L1BlockReference,
    L1Event,
    PriceSample,
};
use starknet_api::block::{BlockHashAndNumber, BlockNumber, BlockTimestamp};
use thiserror::Error;

use crate::l1_gas_price_provider::{L1GasPriceProviderClient, L1GasPriceProviderError};
use crate::l1_gas_price_scraper::{L1GasPriceScraper, L1GasPriceScraperConfig};

const BLOCK_TIME: u64 = 2;
const MEAN_GAS_PRICE: u128 = 42;
const MEAN_DATA_PRICE: u128 = 137;

#[derive(thiserror::Error, Debug)]
pub enum MockBaseLayerError {}

#[derive(Debug)]
struct MockBaseLayerContract {
    time_between_blocks: u64,
    gas_price: u128,
    data_price: u128,
    last_block_num: L1BlockNumber,
}

impl Default for MockBaseLayerContract {
    fn default() -> Self {
        Self {
            time_between_blocks: BLOCK_TIME,
            gas_price: MEAN_GAS_PRICE,
            data_price: MEAN_DATA_PRICE,
            last_block_num: 0,
        }
    }
}

#[async_trait]
impl BaseLayerContract for MockBaseLayerContract {
    type Error = MockBaseLayerError;
    async fn get_price_sample(
        &self,
        block_num: L1BlockNumber,
    ) -> Result<Option<PriceSample>, Self::Error> {
        Ok(Some(PriceSample {
            timestamp: block_num * self.time_between_blocks,
            base_fee_per_gas: self.gas_price,
            blob_fee: self.data_price,
        }))
    }

    async fn latest_l1_block_number(&self, _: u64) -> Result<Option<L1BlockNumber>, Self::Error> {
        Ok(Some(self.last_block_num))
    }

    async fn get_proved_block_at(
        &self,
        _: L1BlockNumber,
    ) -> Result<BlockHashAndNumber, Self::Error> {
        todo!();
    }

    async fn latest_proved_block(&self, _: u64) -> Result<Option<BlockHashAndNumber>, Self::Error> {
        todo!();
    }

    async fn latest_l1_block(&self, _: u64) -> Result<Option<L1BlockReference>, Self::Error> {
        todo!();
    }

    async fn l1_block_at(&self, _: L1BlockNumber) -> Result<Option<L1BlockReference>, Self::Error> {
        todo!();
    }

    /// Get specific events from the Starknet base contract between two L1 block numbers.
    async fn events(
        &self,
        _: RangeInclusive<L1BlockNumber>,
        _: &[&str],
    ) -> Result<Vec<L1Event>, Self::Error> {
        todo!();
    }
}

#[derive(Debug, Default)]
struct MockL1GasPriceProvider {
    block_numbers: Vec<u64>,
    timestamps: Vec<u64>,
    base_fees: Vec<u128>,
    data_fees: Vec<u128>,
}

impl L1GasPriceProviderClient for MockL1GasPriceProvider {
    fn add_price_info(
        &mut self,
        height: BlockNumber,
        timestamp: BlockTimestamp,
        gas_price: u128,
        data_gas_price: u128,
    ) -> Result<(), L1GasPriceProviderError> {
        self.block_numbers.push(height.0);
        self.timestamps.push(timestamp.0);
        self.base_fees.push(gas_price);
        self.data_fees.push(data_gas_price);
        Ok(())
    }

    fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> Result<(u128, u128), L1GasPriceProviderError> {
        let index = self.timestamps.iter().position(|&x| x == timestamp.0).unwrap();
        Ok((self.base_fees[index], self.data_fees[index]))
    }
}

#[tokio::test]
async fn run_l1_gas_price_scraper() {
    let mut mock_contract = MockBaseLayerContract::default();
    let mut mock_provider = MockL1GasPriceProvider::default();

    let mut scraper = L1GasPriceScraper::new(
        L1GasPriceScraperConfig {
            polling_interval: Duration::from_millis(1),
            ..Default::default()
        },
        mock_provider,
        mock_contract,
    );

    // Run the scraper as a separate task.
    let _ = tokio::spawn(async move {
        scraper.run().await.unwrap();
    });

    // Let it run a little bit.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The provider should have received nothing so far.
    assert_eq!(mock_provider.block_numbers.len(), 0);

    // Add a few blocks to the contract.
    let number = 10;
    mock_contract.last_block_num = number;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The provider should have received the blocks.
    assert_eq!(mock_provider.block_numbers, (0..=number).collect::<Vec<u64>>());
    assert_eq!(
        mock_provider.timestamps,
        (0..=number).map(|x| x * BLOCK_TIME).collect::<Vec<u64>>()
    );
    assert_eq!(mock_provider.base_fees, vec![MEAN_GAS_PRICE; number as usize + 1]);
    assert_eq!(mock_provider.data_fees, vec![MEAN_DATA_PRICE; number as usize + 1]);

    // Change the pricing and add one more block.
    mock_contract.gas_price = 100;
    mock_contract.data_price = 200;
    mock_contract.last_block_num = number + 1;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The provider should have received the new block.
    assert_eq!(mock_provider.block_numbers, (0..=number + 1).collect::<Vec<u64>>());
    assert_eq!(mock_provider.timestamps[(number + 1) as usize], (number + 1) * BLOCK_TIME);
    assert_eq!(mock_provider.base_fees[(number + 1) as usize], 100);
    assert_eq!(mock_provider.data_fees[(number + 1) as usize], 200);
}
