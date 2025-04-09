use std::collections::{BTreeMap, VecDeque};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_l1_gas_price_types::errors::L1GasPriceProviderError;
use apollo_l1_gas_price_types::{L1GasPriceProviderResult, PriceInfo};
use papyrus_base_layer::{L1BlockNumber, PriceSample};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockTimestamp, TEMP_ETH_BLOB_GAS_FEE_IN_WEI, TEMP_ETH_GAS_FEE_IN_WEI};
use tracing::warn;
use validator::Validate;

#[cfg(test)]
#[path = "l1_gas_price_provider_test.rs"]
pub mod l1_gas_price_provider_test;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceProviderConfig {
    // TODO(guyn): these two fields need to go into VersionedConstants.
    pub number_of_blocks_for_mean: u64,
    // Use seconds not Duration since seconds is the basic quanta of time for both Starknet and
    // Ethereum.
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

impl SerializeConfig for L1GasPriceProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "number_of_blocks_for_mean",
                &self.number_of_blocks_for_mean,
                "Number of blocks to use for the mean gas price calculation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "lag_margin_seconds",
                &self.lag_margin_seconds,
                "Difference between the time of the block from L1 used to calculate the gas price \
                 and the time of the L2 block this price is used in",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "storage_limit",
                &self.storage_limit,
                "Maximum number of L1 blocks to keep cached",
                ParamPrivacyInput::Public,
            ),
        ])
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

    pub fn make_new_provider_with_fake_data(config: L1GasPriceProviderConfig) -> Self {
        let number = config.number_of_blocks_for_mean;
        let mut provider = Self::new(config);
        for h in 0..number {
            provider
                .add_price_info(
                    h,
                    PriceSample {
                        timestamp: h,
                        base_fee_per_gas: TEMP_ETH_GAS_FEE_IN_WEI,
                        blob_fee: TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
                    },
                )
                .expect("Could not post price sample");
        }
        provider
    }

    pub fn add_price_info(
        &mut self,
        height: L1BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderResult<()> {
        if let Some(data) = self.price_samples_by_block.back() {
            if height != data.height + 1 {
                return Err(L1GasPriceProviderError::UnexpectedHeightError {
                    expected: data.height + 1,
                    found: height,
                });
            }
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
        // The `last_index` is the index of the last block that satisfies the timestamp-lag
        // condition.
        let last_index = self.price_samples_by_block.len() - last_index_rev;

        let num_blocks = usize::try_from(self.config.number_of_blocks_for_mean)
            .expect("number_of_blocks_for_mean is too large to fit into a usize");
        // Can be one of two cases:
        // - not enough history: use blocks from start up to `last_index`. `first_index = 0`.
        // - enough history: take exactly `num_blocks`, `first_index = last_index - num_blocks`.
        let first_index = last_index.saturating_sub(num_blocks);
        if last_index < num_blocks {
            warn!(
                "Not enough history to calculate the mean gas price. Using only {} blocks instead \
                 of {}.",
                last_index, num_blocks
            );
        }
        // Go over all elements between `first_index` to `last_index` (non-inclusive).
        let (gas_price, data_gas_price) =
            self.price_samples_by_block.iter().skip(first_index).take(num_blocks).fold(
                (0, 0),
                |(sum_base, sum_blob), data| {
                    (sum_base + data.sample.base_fee_per_gas, sum_blob + data.sample.blob_fee)
                },
            );
        let actual_number_of_blocks =
            u128::try_from(last_index - first_index).expect("Cannot convert to u128");
        Ok(PriceInfo {
            base_fee_per_gas: gas_price / actual_number_of_blocks,
            blob_fee: data_gas_price / actual_number_of_blocks,
        })
    }
}

impl ComponentStarter for L1GasPriceProvider {}
