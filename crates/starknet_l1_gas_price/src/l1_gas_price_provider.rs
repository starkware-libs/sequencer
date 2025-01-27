use std::collections::VecDeque;

use papyrus_base_layer::PriceSample;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use thiserror::Error;
use validator::Validate;

#[cfg(test)]
#[path = "l1_gas_price_provider_test.rs"]
pub mod l1_gas_price_provider_test;

// TODO(guyn, Gilad): consider moving this to starknet_l1_provider_types/lib.rs?
// This is an interface that allows sharing the provider with the scraper across threads.
pub trait L1GasPriceProviderClient: Send + Sync {
    fn add_price_info(
        &self,
        height: BlockNumber,
        timestamp: BlockTimestamp,
        gas_price: u128,
        data_gas_price: u128,
    ) -> Result<(), L1GasPriceProviderError>;

    fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> Result<(u128, u128), L1GasPriceProviderError>;
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    InvalidHeight(String),
    #[error("Failed to add price info: {0}")]
    MissingData(String),
    #[error("Failed to get price info: {0}")]
    GetPriceInfoError(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceProviderConfig {
    // TODO(guyn): these two fields need to go into VersionedConstants.
    pub number_of_blocks_for_mean: u64,
    pub lag_margin_seconds: u64,
    pub storage_limit: usize,
}

impl Default for L1GasPriceProviderConfig {
    fn default() -> Self {
        const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
        Self {
            number_of_blocks_for_mean: MEAN_NUMBER_OF_BLOCKS,
            lag_margin_seconds: 60,
            storage_limit: usize::try_from(10 * MEAN_NUMBER_OF_BLOCKS).unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct L1GasPriceProvider {
    config: L1GasPriceProviderConfig,
    data: VecDeque<(BlockNumber, PriceSample)>,
}

// TODO(guyn): remove the dead code attribute when we use this.
#[allow(dead_code)]
impl L1GasPriceProvider {
    pub fn new(config: L1GasPriceProviderConfig) -> Self {
        Self { config, data: VecDeque::new() }
    }

    pub fn add_price_info(
        &mut self,
        height: BlockNumber,
        sample: PriceSample,
    ) -> Result<(), L1GasPriceProviderError> {
        let last_plus_one = self.data.back().map(|(h, _)| h.0 + 1).unwrap_or(0);
        if height.0 != last_plus_one {
            return Err(L1GasPriceProviderError::InvalidHeight(format!(
                "Block height is not consecutive: expected {}, got {}",
                last_plus_one, height.0
            )));
        }
        self.data.push_back((height, sample));
        if self.data.len() > self.config.storage_limit {
            self.data.pop_front();
        }
        Ok(())
    }

    pub fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> Result<(u128, u128), L1GasPriceProviderError> {
        let mut gas_price = 0;
        let mut data_gas_price = 0;

        // This index is for the last block in the mean (inclusive).
        let index_last_timestamp_rev = self.data.iter().rev().position(|(_, sample)| {
            sample.timestamp <= timestamp.0 - self.config.lag_margin_seconds
        });

        // Could not find a block with the requested timestamp and lag.
        let Some(last_index_rev) = index_last_timestamp_rev else {
            return Err(L1GasPriceProviderError::MissingData(format!(
                "No block price data from time {} - {}s",
                timestamp.0, self.config.lag_margin_seconds
            )));
        };
        // We need to convert the index to the forward direction.
        let last_index = self.data.len() - last_index_rev;

        let num_blocks = usize::try_from(self.config.number_of_blocks_for_mean)
            .expect("number_of_blocks_for_mean is too large to fit into a usize");
        if last_index < num_blocks {
            return Err(L1GasPriceProviderError::MissingData(format!(
                "Insufficient block price history: expected at least {}, found only {}",
                num_blocks, last_index
            )));
        }
        // Go over all elements between last_index-num_blocks to last_index (non-inclusive).
        for (_height, sample) in self.data.iter().skip(last_index - num_blocks).take(num_blocks) {
            gas_price += sample.base_fee_per_gas;
            data_gas_price += sample.blob_fee;
        }
        Ok((
            gas_price / u128::from(self.config.number_of_blocks_for_mean),
            data_gas_price / u128::from(self.config.number_of_blocks_for_mean),
        ))
    }
}
