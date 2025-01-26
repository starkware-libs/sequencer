use std::sync::Arc;
use std::time::Duration;

use papyrus_base_layer::BaseLayerContract;
use papyrus_config::converters::deserialize_float_seconds_to_duration;
use papyrus_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;
use tracing::error;
use validator::Validate;

use crate::l1_gas_price_provider::{L1GasPriceProviderClient, L1GasPriceProviderError};

#[cfg(test)]
#[path = "l1_gas_price_scraper_test.rs"]
pub mod l1_gas_price_scraper_test;

type L1GasPriceScraperResult<T, B> = Result<T, L1GasPriceScraperError<B>>;
pub type SharedL1GasPriceProvider = Arc<dyn L1GasPriceProviderClient>;

#[derive(Error, Debug)]
pub enum L1GasPriceScraperError<T: BaseLayerContract + Send + Sync> {
    #[error("Base layer error: {0}")]
    BaseLayerError(T::Error),
    #[error("Could not update gas price provider: {0}")]
    GasPriceProviderError(L1GasPriceProviderError),
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
}

// TODO(guyn): find a way to synchronize the value of number_of_blocks_for_mean
// with the one in L1GasPriceProviderConfig. In the end they should both be loaded
// from VersionedConstants.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceScraperConfig {
    pub starting_block: Option<u64>,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub polling_interval: Duration,
    pub number_of_blocks_for_mean: u64,
}

impl Default for L1GasPriceScraperConfig {
    fn default() -> Self {
        Self {
            // Default is to start scraping from 2*number_of_blocks_for_mean before latest.
            // Use this config field to override this starting point.
            starting_block: None,
            chain_id: ChainId::Other("0x0".to_string()),
            finality: 3,
            polling_interval: Duration::from_secs(1),
            number_of_blocks_for_mean: 300,
        }
    }
}
pub struct L1GasPriceScraper<B: BaseLayerContract> {
    pub config: L1GasPriceScraperConfig,
    pub base_layer: B,
    pub next_block_number_to_fetch: u64,
    pub l1_gas_price_provider: SharedL1GasPriceProvider,
}

impl<B: BaseLayerContract + Send + Sync> L1GasPriceScraper<B> {
    pub fn new(
        config: L1GasPriceScraperConfig,
        l1_gas_price_provider: SharedL1GasPriceProvider,
        base_layer: B,
    ) -> Self {
        Self { l1_gas_price_provider, base_layer, next_block_number_to_fetch: 0, config }
    }

    // TODO(guyn): dead code can be removed when adding the component starter in the next PR.
    #[allow(dead_code)]
    async fn run(&mut self) -> L1GasPriceScraperResult<(), B> {
        // Choose the oldest block we need to fetch.
        // It is either self.config.starting_block, or the current head of the chain,
        // minus 2 * MEAN_NUMBER_OF_BLOCKS. Note that this minus can be less than zero
        // for short chains, hence the saturating_sub.
        self.next_block_number_to_fetch = match self.config.starting_block {
            Some(start) => start,
            None => self
                .base_layer
                .latest_l1_block_number(self.config.finality)
                .await
                .expect("Failed to get latest block number")
                .expect("Failed to get latest block number")
                .saturating_sub(2 * self.config.number_of_blocks_for_mean),
        };
        loop {
            self.update_prices().await?;
            tokio::time::sleep(self.config.polling_interval).await;
        }
    }

    async fn update_prices(&mut self) -> L1GasPriceScraperResult<(), B> {
        loop {
            let price_sample = self
                .base_layer
                .get_price_sample(self.next_block_number_to_fetch)
                .await
                .map_err(L1GasPriceScraperError::BaseLayerError)?;
            match price_sample {
                None => break,
                Some(sample) => {
                    self.l1_gas_price_provider
                        .add_price_info(BlockNumber(self.next_block_number_to_fetch), sample)
                        .map_err(L1GasPriceScraperError::GasPriceProviderError)?;

                    self.next_block_number_to_fetch += 1;
                }
            }
        }

        Ok(())
    }
}
