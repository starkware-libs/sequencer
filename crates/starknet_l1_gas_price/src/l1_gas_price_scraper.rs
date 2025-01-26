use std::any::type_name;
use std::cmp::max;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_base_layer::BaseLayerContract;
use papyrus_config::converters::deserialize_float_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::validators::validate_ascii;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ChainId;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use thiserror::Error;
use tracing::{error, info};
use validator::Validate;

use crate::l1_gas_price_provider::{
    L1GasPriceProviderClient,
    L1GasPriceProviderError,
    MEAN_NUMBER_OF_BLOCKS,
};

#[cfg(test)]
#[path = "l1_gas_price_scraper_tests.rs"]
pub mod l1_gas_price_scraper_tests;

type L1GasPriceScraperResult<T, B> = Result<T, L1GasPriceScraperError<B>>;
pub type SharedL1GasPriceProvider = Arc<dyn L1GasPriceProviderClient>;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceScraperConfig {
    pub l1_block_to_start_scraping_from: u64,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub polling_interval: Duration,
}

impl Default for L1GasPriceScraperConfig {
    fn default() -> Self {
        Self {
            l1_block_to_start_scraping_from: 0,
            chain_id: ChainId::Mainnet,
            finality: 0,
            polling_interval: Duration::from_secs(1),
        }
    }
}

impl SerializeConfig for L1GasPriceScraperConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "l1_block_to_start_scraping_from",
                &0,
                "Last L1 block number processed by the scraper",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "finality",
                &3,
                "Number of blocks to wait for finality",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "polling_interval",
                &self.polling_interval.as_secs(),
                "Interval in Seconds between each scraping attempt of L1.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

pub struct L1GasPriceScraper<B: BaseLayerContract> {
    pub config: L1GasPriceScraperConfig,
    pub base_layer: B,
    pub last_block_number_processed: u64,
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
            last_block_number_processed: config.l1_block_to_start_scraping_from,
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
        if let Some(latest_l1_block_number) = latest_l1_block_number {
            if self.last_block_number_processed >= latest_l1_block_number {
                // We are already up to date.
                return Ok(());
            }
            let oldest_l1_block_number = max(
                self.last_block_number_processed,
                latest_l1_block_number - 2 * MEAN_NUMBER_OF_BLOCKS,
            );
            for block_number in (oldest_l1_block_number..=latest_l1_block_number).rev() {
                if let Some(sample) = self
                    .base_layer
                    .get_price_sample(block_number)
                    .await
                    .map_err(L1GasPriceScraperError::BaseLayerError)?
                {
                    self.l1_gas_price_provider
                        .add_price_info(
                            BlockNumber(block_number),
                            BlockTimestamp(sample.timestamp),
                            sample.base_fee_per_gas,
                            sample.blob_fee,
                        )
                        .map_err(L1GasPriceScraperError::GasPriceProviderError)?;
                }
                // Only update the last block number processed if we successfully processed all the
                // blocks.
                self.last_block_number_processed = latest_l1_block_number;
            }
        } else {
            error!("Failed to get latest L1 block number, finality too high.");
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
    GasPriceProviderError(L1GasPriceProviderError),
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
}
