use std::any::type_name;
use std::cmp::max;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_base_layer::BaseLayerContract;
use papyrus_config::converters::deserialize_float_seconds_to_duration;
use papyrus_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use thiserror::Error;
use tracing::{error, info};
use validator::Validate;

use crate::{L1GasPriceClientError, L1GasPriceProviderClient};

#[cfg(test)]
#[path = "l1_gas_price_scraper_test.rs"]
pub mod l1_gas_price_scraper_test;

type L1GasPriceScraperResult<T, B> = Result<T, L1GasPriceScraperError<B>>;
pub type SharedL1GasPriceProvider = Arc<dyn L1GasPriceProviderClient>;

// TODO(guyn): find a way to synchronize the value of number_of_blocks_for_mean
// with the one in L1GasPriceProviderConfig. In the end they should not be config
// items but values drawn from VersionedConstants.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceScraperConfig {
    pub l1_block_to_start_scraping_from: u64,
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
            l1_block_to_start_scraping_from: 0,
            chain_id: ChainId::Mainnet,
            finality: 0,
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
        Self {
            l1_gas_price_provider,
            base_layer,
            next_block_number_to_fetch: config.l1_block_to_start_scraping_from,
            config,
        }
    }

    async fn run(&mut self) -> L1GasPriceScraperResult<(), B> {
        loop {
            self.update_prices().await?;
            tokio::time::sleep(self.config.polling_interval).await;
        }
    }

    async fn update_prices(&mut self) -> L1GasPriceScraperResult<(), B> {
        let latest_l1_block_number = self
            .base_layer
            .latest_l1_block_number(self.config.finality)
            .await
            .map_err(L1GasPriceScraperError::BaseLayerError)?;

        let Some(latest_l1_block_number) = latest_l1_block_number else {
            error!("Failed to get latest L1 block number, finality too high.");
            return Ok(());
        };

        if self.next_block_number_to_fetch > latest_l1_block_number {
            // We are already up to date.
            return Ok(());
        }
        // Choose the oldest block we need to fetch.
        // It is either next_block_number_to_fetch, or the current head of the chain,
        // minus 2 * MEAN_NUMBER_OF_BLOCKS. Note that this minus can be less than zero
        // for short chains, hence the saturating_sub.
        let oldest_l1_block_number = max(
            self.next_block_number_to_fetch,
            latest_l1_block_number.saturating_sub(2 * self.config.number_of_blocks_for_mean),
        );
        for block_number in oldest_l1_block_number..=latest_l1_block_number {
            if let Some(sample) = self
                .base_layer
                .get_price_sample(block_number)
                .await
                .map_err(L1GasPriceScraperError::BaseLayerError)?
            {
                self.l1_gas_price_provider
                    .add_price_info(BlockNumber(block_number), sample)
                    .await
                    .map_err(L1GasPriceScraperError::GasPriceClientError)?;

                self.next_block_number_to_fetch = block_number + 1;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> ComponentStarter for L1GasPriceScraper<B> {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|_| ComponentError::InternalComponentError)
    }
}

#[derive(Error, Debug)]
pub enum L1GasPriceScraperError<T: BaseLayerContract + Send + Sync> {
    #[error("Base layer error: {0}")]
    BaseLayerError(T::Error),
    #[error("Could not update gas price provider: {0}")]
    GasPriceClientError(L1GasPriceClientError),
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
}
