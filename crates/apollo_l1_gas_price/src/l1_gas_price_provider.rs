use std::any::type_name;
use std::collections::VecDeque;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::info_every_n_sec;
use apollo_l1_gas_price_types::errors::L1GasPriceProviderError;
use apollo_l1_gas_price_types::{GasPriceData, L1GasPriceProviderResult, PriceInfo};
use async_trait::async_trait;
use starknet_api::block::BlockTimestamp;
use tracing::{info, trace, warn};

use crate::config::L1GasPriceProviderConfig;
use crate::metrics::{
    register_provider_metrics,
    L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE,
    L1_GAS_PRICE_LATEST_MEAN_VALUE,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
};

#[cfg(test)]
#[path = "l1_gas_price_provider_test.rs"]
pub mod l1_gas_price_provider_test;

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
pub struct L1GasPriceProvider {
    config: L1GasPriceProviderConfig,
    // If received data before initialization (is None), it means the scraper has restarted.
    price_samples_by_block: Option<RingBuffer<GasPriceData>>,
}

impl L1GasPriceProvider {
    pub fn new(config: L1GasPriceProviderConfig) -> Self {
        Self { config, price_samples_by_block: None }
    }

    pub fn initialize(&mut self) -> L1GasPriceProviderResult<()> {
        info!("Initializing L1GasPriceProvider with config: {:?}", self.config);
        self.price_samples_by_block = Some(RingBuffer::new(self.config.storage_limit));
        Ok(())
    }

    pub fn add_price_info(&mut self, new_data: GasPriceData) -> L1GasPriceProviderResult<()> {
        // In case the provider has been restarted while the scraper is still running,
        // a NotInitializedError will be returned to the scraper. We expect the scraper to exit with
        // an error, and that infrastructure will restart it, leading to initialization.
        let Some(samples) = &mut self.price_samples_by_block else {
            return Err(L1GasPriceProviderError::NotInitializedError);
        };
        if let Some(data) = samples.back() {
            if new_data.block_number != data.block_number + 1 {
                return Err(L1GasPriceProviderError::UnexpectedBlockNumberError {
                    expected: data.block_number + 1,
                    found: new_data.block_number,
                });
            }
        }
        trace!("Received price sample for L1 block: {:?}", new_data);
        info_every_n_sec!(1, "Received price sample for L1 block: {:?}", new_data);
        samples.push(new_data);
        Ok(())
    }

    pub fn get_price_info(&self, timestamp: BlockTimestamp) -> L1GasPriceProviderResult<PriceInfo> {
        let Some(samples) = &self.price_samples_by_block else {
            return Err(L1GasPriceProviderError::NotInitializedError);
        };
        // timestamp of the newest price sample
        let last_timestamp = samples
            .back()
            .ok_or(L1GasPriceProviderError::MissingDataError {
                timestamp: timestamp.0,
                lag: self.config.lag_margin_seconds,
            })?
            .timestamp;

        // Check if the prices are stale.
        if timestamp.0 > (*last_timestamp + self.config.max_time_gap_seconds) {
            return Err(L1GasPriceProviderError::StaleL1GasPricesError {
                current_timestamp: timestamp.0,
                last_valid_price_timestamp: *last_timestamp,
            });
        }

        // This index is for the last block in the mean (inclusive).
        let index_last_timestamp_rev = samples.iter().rev().position(|data| {
            data.timestamp <= timestamp.saturating_sub(&self.config.lag_margin_seconds)
        });

        // Could not find a block with the requested timestamp and lag.
        let Some(last_index_rev) = index_last_timestamp_rev else {
            return Err(L1GasPriceProviderError::MissingDataError {
                timestamp: timestamp.0,
                lag: self.config.lag_margin_seconds,
            });
        };
        // Convert the index to the forward direction.
        // `last_index` should be one past the final entry we will include in our calculation.
        // The index returned from `position` is guaranteed to be less than `len()`,
        // so `last_index` is guaranteed to be >= 1.
        let last_index = samples.len() - last_index_rev;

        let num_blocks = usize::try_from(self.config.number_of_blocks_for_mean)
            .expect("number_of_blocks_for_mean is too large to fit into a usize");

        let first_index = if last_index >= num_blocks {
            last_index - num_blocks
        } else {
            warn!(
                "Not enough history to calculate the mean gas price. Using blocks {}-{}, \
                 inclusive.",
                samples[0].block_number,
                samples[last_index - 1].block_number,
            );
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.increment(1);
            0
        };
        debug_assert!(first_index < last_index, "error calculating indices");
        let actual_number_of_blocks = last_index - first_index;

        // Go over all elements between `first_index` and `last_index` (non-inclusive).
        let price_info_summed: PriceInfo = samples
            .iter()
            .skip(first_index)
            .take(actual_number_of_blocks)
            .map(|data| &data.price_info)
            .sum();
        let actual_number_of_blocks =
            u128::try_from(actual_number_of_blocks).expect("Cannot convert to u128");
        let price_info_out = price_info_summed
            .checked_div(actual_number_of_blocks)
            .expect("Actual number of blocks should be non-zero");
        info_every_n_sec!(
            1,
            "Calculated L1 gas price for timestamp {}: {:?} (based on blocks {}-{}, inclusive)",
            timestamp.0,
            price_info_out,
            samples[first_index].block_number,
            samples[last_index - 1].block_number,
        );
        L1_GAS_PRICE_LATEST_MEAN_VALUE.set_lossy(price_info_out.base_fee_per_gas.0);
        L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE.set_lossy(price_info_out.blob_fee.0);
        Ok(price_info_out)
    }
}

#[async_trait]
impl ComponentStarter for L1GasPriceProvider {
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        register_provider_metrics();
    }
}
