use std::any::type_name;
use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::sync::Arc;

use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::info_every_n_ms;
use apollo_l1_gas_price_config::config::{
    ChainlinkOracleConfig,
    ExchangeRateOracleConfig,
    ExchangeRateOracleSource,
    L1GasPriceProviderConfig,
};
use apollo_l1_gas_price_types::errors::L1GasPriceProviderError;
use apollo_l1_gas_price_types::{
    EthToFri,
    ExchangeRate,
    ExchangeRateOracleClientTrait,
    GasPriceData,
    L1GasPriceProviderResult,
    PriceInfo,
    StrkToUsd,
};
use async_trait::async_trait;
use starknet_api::block::BlockTimestamp;
use thiserror::Error;
use tracing::{info, trace, warn};

use crate::chainlink_oracle::{ChainlinkOracleClient, ChainlinkRate};
use crate::exchange_rate_oracle::ExchangeRateOracleClient;
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
struct RingBuffer<T> {
    queue: VecDeque<T>,
    limit: usize,
}
impl<T: Clone> RingBuffer<T> {
    fn new(limit: usize) -> Self {
        Self { queue: VecDeque::with_capacity(limit), limit }
    }

    fn push(&mut self, item: T) {
        if self.queue.len() >= self.limit {
            self.queue.pop_front();
        }
        self.queue.push_back(item);
    }
}
// Deref lets us use .iter() and .back(), etc.
// Do not implement mut_deref, as that could break the
// size restriction of the RingBuffer.
impl<T: Clone> std::ops::Deref for RingBuffer<T> {
    type Target = VecDeque<T>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

/// The config keys an operator edits to pick a feed's source. They appear in
/// `MissingBatcherClientError`, so the message names the key as it is written in the config file.
const ETH_TO_STRK_ORACLE_SOURCE_CONFIG_KEY: &str =
    "l1_gas_price_provider_config.eth_to_strk_oracle_source";
const STRK_TO_USD_ORACLE_SOURCE_CONFIG_KEY: &str =
    "l1_gas_price_provider_config.strk_to_usd_oracle_source";

/// Whether a batcher client is available is decided by the composition root, not by config, so
/// this is raised while the components are built rather than caught by config validation.
#[derive(Debug, Eq, Error, PartialEq)]
#[error(
    "Chainlink is selected as the oracle source by {}, but this service has no batcher client. \
     Chainlink feeds are read through the batcher, so either run the batcher alongside this \
     service or set each listed key to Http.",
    .source_config_keys.join(" and ")
)]
pub struct MissingBatcherClientError {
    /// Every feed that selects `Chainlink`, so that one startup reports the whole
    /// misconfiguration.
    source_config_keys: Vec<&'static str>,
}

#[derive(Clone, Debug)]
pub struct L1GasPriceProvider {
    config: L1GasPriceProviderConfig,
    // If received data before initialization (is None), it means the scraper has restarted.
    price_samples_by_block: Option<RingBuffer<GasPriceData>>,
    eth_to_strk_oracle_client: Arc<dyn ExchangeRateOracleClientTrait>,
    strk_to_usd_oracle_client: Arc<dyn ExchangeRateOracleClientTrait>,
}

impl L1GasPriceProvider {
    pub fn new(
        config: L1GasPriceProviderConfig,
        eth_to_strk_oracle_client: Arc<dyn ExchangeRateOracleClientTrait>,
        strk_to_usd_oracle_client: Arc<dyn ExchangeRateOracleClientTrait>,
    ) -> Self {
        Self {
            config,
            price_samples_by_block: None,
            eth_to_strk_oracle_client,
            strk_to_usd_oracle_client,
        }
    }

    /// Builds each feed's oracle client from the source selected for it in `config`.
    /// `batcher_client` is `None` in topologies that run the provider without a batcher, which only
    /// the `Http` source tolerates.
    pub fn new_with_oracle(
        config: L1GasPriceProviderConfig,
        batcher_client: Option<SharedBatcherClient>,
    ) -> Result<Self, MissingBatcherClientError> {
        // Both feeds are resolved before either failure is reported, so an operator who
        // misconfigured both learns of both in one startup.
        let eth_to_strk_oracle_client = build_exchange_rate_oracle_client::<EthToFri>(
            config.eth_to_strk_oracle_source,
            &config.eth_to_strk_oracle_config,
            ETH_TO_STRK_ORACLE_SOURCE_CONFIG_KEY,
            &config.chainlink_oracle_config,
            batcher_client.as_ref(),
        );
        let strk_to_usd_oracle_client = build_exchange_rate_oracle_client::<StrkToUsd>(
            config.strk_to_usd_oracle_source,
            &config.strk_to_usd_oracle_config,
            STRK_TO_USD_ORACLE_SOURCE_CONFIG_KEY,
            &config.chainlink_oracle_config,
            batcher_client.as_ref(),
        );
        match (eth_to_strk_oracle_client, strk_to_usd_oracle_client) {
            (Ok(eth_to_strk_oracle_client), Ok(strk_to_usd_oracle_client)) => {
                Ok(Self::new(config, eth_to_strk_oracle_client, strk_to_usd_oracle_client))
            }
            (eth_to_strk_result, strk_to_usd_result) => Err(MissingBatcherClientError {
                source_config_keys: [eth_to_strk_result.err(), strk_to_usd_result.err()]
                    .into_iter()
                    .flatten()
                    .collect(),
            }),
        }
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
        info_every_n_ms!(1_000, "Received price sample for L1 block: {:?}", new_data);
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
                lag: self.config.lag_margin_seconds.as_secs(),
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
            data.timestamp <= timestamp.saturating_sub(&self.config.lag_margin_seconds.as_secs())
        });

        // Could not find a block with the requested timestamp and lag.
        let Some(last_index_rev) = index_last_timestamp_rev else {
            return Err(L1GasPriceProviderError::MissingDataError {
                timestamp: timestamp.0,
                lag: self.config.lag_margin_seconds.as_secs(),
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
        info_every_n_ms!(
            1_000,
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

    pub async fn eth_to_fri_rate(&self, timestamp: u64) -> L1GasPriceProviderResult<ExchangeRate> {
        self.eth_to_strk_oracle_client
            .fetch_rate(timestamp)
            .await
            .map_err(L1GasPriceProviderError::ExchangeRateOracleClientError)
    }

    pub async fn strk_to_usd_rate(&self, timestamp: u64) -> L1GasPriceProviderResult<ExchangeRate> {
        self.strk_to_usd_oracle_client
            .fetch_rate(timestamp)
            .await
            .map_err(L1GasPriceProviderError::ExchangeRateOracleClientError)
    }
}

/// Builds the client of the single feed `Kind` names, from that feed's own `source`, `http_config`
/// and `source_config_key`. The metrics bundle and the Chainlink read come from `Kind`, so one
/// feed's client can only ever serve that feed's rate.
///
/// `Err` carries `source_config_key` when the feed selects `Chainlink` while no batcher client is
/// available.
fn build_exchange_rate_oracle_client<Kind: ChainlinkRate>(
    source: ExchangeRateOracleSource,
    http_config: &ExchangeRateOracleConfig,
    source_config_key: &'static str,
    chainlink_oracle_config: &ChainlinkOracleConfig,
    batcher_client: Option<&SharedBatcherClient>,
) -> Result<Arc<dyn ExchangeRateOracleClientTrait>, &'static str> {
    let pair = Kind::PAIR;
    info!("Building the {pair:?} exchange rate oracle client from source {source:?}");
    match source {
        ExchangeRateOracleSource::Http => {
            Ok(Arc::new(ExchangeRateOracleClient::new(http_config.clone(), Kind::metrics())))
        }
        ExchangeRateOracleSource::Chainlink => {
            let batcher_client = batcher_client.ok_or(source_config_key)?;
            // The feed samples on the same interval its HTTP client would have, so switching a
            // feed's source changes where the rate comes from and not how often it is taken.
            let sampling_interval_seconds = NonZeroU64::new(http_config.lag_interval_seconds)
                .expect("lag_interval_seconds is validated to be non-zero");
            Ok(Arc::new(ChainlinkOracleClient::<Kind>::new(
                chainlink_oracle_config.clone(),
                sampling_interval_seconds,
                batcher_client.clone(),
            )))
        }
    }
}

#[async_trait]
impl ComponentStarter for L1GasPriceProvider {
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        register_provider_metrics();
    }
}
