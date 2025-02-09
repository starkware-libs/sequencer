use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
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

use crate::l1_gas_price_scraper::{L1GasPriceScraper, L1GasPriceScraperConfig};
use crate::{L1GasPriceProviderClient, L1GasPriceProviderClientResult, L1GasPriceProviderError};

const BLOCK_TIME: u64 = 2;
const GAS_PRICE: u128 = 42;
const DATA_PRICE: u128 = 137;

#[derive(thiserror::Error, Debug)]
pub enum FakeBaseLayerError {}

#[derive(Debug, Clone)]
struct FakeInternalBaseLayerData {
    time_between_blocks: u64,
    gas_price: u128,
    data_price: u128,
    last_block_num: L1BlockNumber,
}

impl Default for FakeInternalBaseLayerData {
    fn default() -> Self {
        Self {
            time_between_blocks: BLOCK_TIME,
            gas_price: GAS_PRICE,
            data_price: DATA_PRICE,
            last_block_num: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct FakeBaseLayerContract {
    data: Arc<Mutex<FakeInternalBaseLayerData>>,
}

impl Default for FakeBaseLayerContract {
    fn default() -> Self {
        Self { data: Arc::new(Mutex::new(FakeInternalBaseLayerData::default())) }
    }
}

#[async_trait]
impl BaseLayerContract for FakeBaseLayerContract {
    type Error = FakeBaseLayerError;
    async fn get_price_sample(
        &self,
        block_num: L1BlockNumber,
    ) -> Result<Option<PriceSample>, Self::Error> {
        let data = self.data.lock().unwrap();
        Ok(Some(PriceSample {
            timestamp: block_num * data.time_between_blocks,
            base_fee_per_gas: data.gas_price,
            blob_fee: data.data_price,
        }))
    }

    async fn latest_l1_block_number(&self, _: u64) -> Result<Option<L1BlockNumber>, Self::Error> {
        Ok(Some(self.data.lock().unwrap().last_block_num))
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

#[allow(clippy::type_complexity)]
#[derive(Debug, Default)]
struct FakeL1GasPriceProvider {
    data: Arc<Mutex<Vec<(BlockNumber, PriceSample)>>>,
}

#[async_trait]
impl L1GasPriceProviderClient for FakeL1GasPriceProvider {
    async fn add_price_info(
        &self,
        height: BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderClientResult<()> {
        self.data.lock().unwrap().push((height, sample));
        Ok(())
    }

    async fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> L1GasPriceProviderClientResult<(u128, u128)> {
        let vector = self.data.lock().unwrap();
        let index = vector.iter().position(|(_, sample)| sample.timestamp >= timestamp.0).unwrap();
        Ok((vector[index].1.base_fee_per_gas, vector[index].1.blob_fee))
    }
}

#[tokio::test]
#[allow(clippy::as_conversions)]
async fn run_l1_gas_price_scraper() {
    let fake_contract = FakeBaseLayerContract::default();
    let fake_provider = Arc::new(FakeL1GasPriceProvider::default());

    let mut scraper = L1GasPriceScraper::new(
        L1GasPriceScraperConfig {
            polling_interval: Duration::from_millis(1),
            ..Default::default()
        },
        fake_provider.clone(),
        fake_contract.clone(),
    );

    // Run the scraper as a separate task in the background.
    let _future_handle = tokio::spawn(async move {
        scraper.run().await.unwrap();
    });

    // Let the scraper have some time to work.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // There is only block zero on the contract.
    assert_eq!(fake_provider.data.lock().unwrap().len(), 1);
    {
        let data = fake_provider.data.lock().unwrap();
        assert_eq!(data[0].0.0, 0);
        assert_eq!(data[0].1.timestamp, 0);
        assert_eq!(data[0].1.base_fee_per_gas, GAS_PRICE);
        assert_eq!(data[0].1.blob_fee, DATA_PRICE);
    } // Inner scope ends here to release the lock.

    // Add a few blocks to the contract.
    let number = 10;
    {
        fake_contract.data.lock().unwrap().last_block_num = number;
    } // Inner scope ends here to release the lock.

    // Let the scraper have some time to work.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The provider should have received the blocks.
    {
        let data = fake_provider.data.lock().unwrap();
        let block_numbers = data.iter().map(|(height, _)| height.0).collect::<Vec<u64>>();
        let timestamps = data.iter().map(|(_, sample)| sample.timestamp).collect::<Vec<u64>>();
        let gas_prices =
            data.iter().map(|(_, sample)| sample.base_fee_per_gas).collect::<Vec<u128>>();
        let data_prices = data.iter().map(|(_, sample)| sample.blob_fee).collect::<Vec<u128>>();

        assert_eq!(block_numbers, (0..=number).collect::<Vec<u64>>());
        assert_eq!(timestamps, (0..=number).map(|x| x * BLOCK_TIME).collect::<Vec<u64>>());
        assert_eq!(gas_prices, vec![GAS_PRICE; number as usize + 1]);
        assert_eq!(data_prices, vec![DATA_PRICE; number as usize + 1]);
    } // Inner scope ends here to release the lock.

    // Change the pricing and add one more block.
    {
        let mut contract_data = fake_contract.data.lock().unwrap();
        contract_data.gas_price = 100;
        contract_data.data_price = 200;
        contract_data.last_block_num = number + 1;
    } // Inner scope ends here to release the lock.

    // Let the scraper have some time to work.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The provider should have received the new block.
    {
        let data = fake_provider.data.lock().unwrap();
        let block_numbers = data.iter().map(|(height, _)| height.0).collect::<Vec<u64>>();
        let last_timestamp = data[(number + 1) as usize].1.timestamp;
        assert_eq!(block_numbers, (0..=number + 1).collect::<Vec<u64>>());
        assert_eq!(last_timestamp, (number + 1) * BLOCK_TIME);
        assert_eq!(data[(number + 1) as usize].1.base_fee_per_gas, 100);
        assert_eq!(data[(number + 1) as usize].1.blob_fee, 200);
    } // Inner scope ends here to release the lock. (in case the test is extended later)
}
