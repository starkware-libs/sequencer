use std::collections::VecDeque;

use papyrus_base_layer::{L1BlockNumber, PriceSample};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockTimestamp;
use starknet_l1_gas_price_types::errors::L1GasPriceProviderError;
use starknet_l1_gas_price_types::{L1GasPriceProviderResult, PriceInfo};
use validator::Validate;

#[cfg(test)]
#[path = "l1_gas_price_provider_test.rs"]
pub mod l1_gas_price_provider_test;

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

#[derive(Clone, Debug, Eq, PartialEq)]
struct RingBuffer<T>(VecDeque<T>);
impl<T: Clone> RingBuffer<T> {
    fn new(size: usize) -> Self {
        Self(VecDeque::with_capacity(size))
    }

    fn push(&mut self, item: T) {
        if self.0.len() == self.0.capacity() {
            self.0.pop_front();
        }
        self.0.push_back(item);
    }
}
// Deref lets us use .iter() and .back(), etc.
// Do not implement mut_deref, as that could break the
// size restriction of the RingBuffer.
impl<T: Clone> std::ops::Deref for RingBuffer<T> {
    type Target = VecDeque<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct GasPriceData {
    pub height: L1BlockNumber,
    pub sample: PriceSample,
}

#[derive(Clone, Debug)]
pub struct L1GasPriceProvider {
    config: L1GasPriceProviderConfig,
    price_samples_by_block: RingBuffer<GasPriceData>,
}

impl L1GasPriceProvider {
    pub fn new(config: L1GasPriceProviderConfig) -> Self {
        let storage_limit = config.storage_limit;
        Self { config, price_samples_by_block: RingBuffer::new(storage_limit) }
    }

    pub fn add_price_info(
        &mut self,
        height: L1BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderResult<()> {
        let last_plus_one =
            self.price_samples_by_block.back().map(|data| data.height + 1).unwrap_or(0);
        if height != last_plus_one {
            return Err(L1GasPriceProviderError::UnexpectedHeightError {
                expected: last_plus_one,
                found: height,
            });
        }
        self.price_samples_by_block.push(GasPriceData { height, sample });
        Ok(())
    }

    pub fn get_price_info(&self, timestamp: BlockTimestamp) -> L1GasPriceProviderResult<PriceInfo> {
        // This index is for the last block in the mean (inclusive).
        let index_last_timestamp_rev =
            self.price_samples_by_block.iter().rev().position(|data| {
                data.sample.timestamp <= timestamp.0 - self.config.lag_margin_seconds
            });

        // Could not find a block with the requested timestamp and lag.
        let Some(last_index_rev) = index_last_timestamp_rev else {
            return Err(L1GasPriceProviderError::MissingDataError {
                timestamp: timestamp.0,
                lag: self.config.lag_margin_seconds,
            });
        };
        // We need to convert the index to the forward direction.
        let last_index = self.price_samples_by_block.len() - last_index_rev;

        let num_blocks = usize::try_from(self.config.number_of_blocks_for_mean)
            .expect("number_of_blocks_for_mean is too large to fit into a usize");
        if last_index < num_blocks {
            return Err(L1GasPriceProviderError::InsufficientHistoryError {
                expected: num_blocks,
                found: last_index,
            });
        }
        // Go over all elements between last_index-num_blocks to last_index (non-inclusive).
        let (gas_price, data_gas_price) =
            self.price_samples_by_block.iter().skip(last_index - num_blocks).take(num_blocks).fold(
                (0, 0),
                |(sum_base, sum_blob), data| {
                    (sum_base + data.sample.base_fee_per_gas, sum_blob + data.sample.blob_fee)
                },
            );

        Ok(PriceInfo {
            base_fee_per_gas: gas_price / u128::from(self.config.number_of_blocks_for_mean),
            blob_fee: data_gas_price / u128::from(self.config.number_of_blocks_for_mean),
        })
    }
}
