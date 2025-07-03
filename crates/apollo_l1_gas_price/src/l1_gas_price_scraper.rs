use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use apollo_config::converters::deserialize_float_seconds_to_duration;
use apollo_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::info_every_n;
use apollo_l1_gas_price_types::errors::L1GasPriceClientError;
use apollo_l1_gas_price_types::{GasPriceData, L1GasPriceProviderClient, PriceInfo};
use async_trait::async_trait;
use papyrus_base_layer::{BaseLayerContract, L1BlockHeader, L1BlockNumber};
use serde::{Deserialize, Serialize};
use starknet_api::block::GasPrice;
use starknet_api::core::ChainId;
use thiserror::Error;
use tracing::{error, info, trace, warn};
use validator::Validate;

use crate::metrics::{
    register_scraper_metrics,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};

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
    GasPriceClientError(L1GasPriceClientError),
    #[error("L1 reorg detected: {reason}. Restart both the L1 gas price provider and scraper.")]
    L1ReorgDetected { reason: String },
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
}

// TODO(guyn): find a way to synchronize the value of number_of_blocks_for_mean
// with the one in L1GasPriceProviderConfig. In the end they should both be loaded
// from VersionedConstants.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceScraperConfig {
    /// This field is ignored by the L1Scraper.
    /// Manual override to specify where the scraper should start.
    /// If None, the node will start scraping from 2*number_of_blocks_for_mean before the tip of
    /// L1.
    pub starting_block: Option<u64>,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub polling_interval: Duration,
    pub number_of_blocks_for_mean: u64,
    // How many sets of config.num_blocks_for_mean blocks to go back
    // on the chain when starting to scrape.
    pub startup_num_blocks_multiplier: u64,
}

impl Default for L1GasPriceScraperConfig {
    fn default() -> Self {
        Self {
            starting_block: None,
            chain_id: ChainId::Other("0x0".to_string()),
            finality: 0,
            polling_interval: Duration::from_secs(1),
            number_of_blocks_for_mean: 300,
            startup_num_blocks_multiplier: 2,
        }
    }
}

impl SerializeConfig for L1GasPriceScraperConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "finality",
                &self.finality,
                "Number of blocks to wait for finality in L1",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "polling_interval",
                &self.polling_interval.as_secs(),
                "The duration (seconds) between each scraping attempt of L1",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "number_of_blocks_for_mean",
                &self.number_of_blocks_for_mean,
                "Number of blocks to use for the mean gas price calculation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "startup_num_blocks_multiplier",
                &self.startup_num_blocks_multiplier,
                "How many sets of config.num_blocks_for_mean blocks to go back on the chain when starting to scrape.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.starting_block,
            0, // This value is never used, since #is_none turns it to a None.
            "starting_block",
            "Starting block to scrape from",
            ParamPrivacyInput::Public,
        ));
        config
    }
}

pub struct L1GasPriceScraper<B: BaseLayerContract> {
    pub config: L1GasPriceScraperConfig,
    pub base_layer: B,
    pub l1_gas_price_provider: SharedL1GasPriceProvider,
    pub last_l1_header: Option<L1BlockHeader>,
}

impl<B: BaseLayerContract + Send + Sync> L1GasPriceScraper<B> {
    pub fn new(
        config: L1GasPriceScraperConfig,
        l1_gas_price_provider: SharedL1GasPriceProvider,
        base_layer: B,
    ) -> Self {
        Self { config, l1_gas_price_provider, base_layer, last_l1_header: None }
    }

    /// Run the scraper, starting from the given L1 `block_number`, indefinitely.
    pub async fn run(&mut self, mut block_number: L1BlockNumber) -> L1GasPriceScraperResult<(), B> {
        self.l1_gas_price_provider
            .initialize()
            .await
            .map_err(L1GasPriceScraperError::GasPriceClientError)?;
        loop {
            block_number = self.update_prices(block_number).await?;
            tokio::time::sleep(self.config.polling_interval).await;
        }
    }

    /// Scrape all blocks the provider knows starting from `block_number`.
    /// Returns the next `block_number` to be scraped.
    async fn update_prices(
        &mut self,
        start_block_number: L1BlockNumber,
    ) -> L1GasPriceScraperResult<L1BlockNumber, B> {
        let Some(last_block_number) = self.latest_l1_block_number().await? else {
            // Not enough blocks under current finality. Try again later.
            return Ok(start_block_number);
        };
        trace!(
            "Scraping gas prices starting from block {start_block_number} to {last_block_number}."
        );
        // TODO(guy.f): Replace with info_every_n_sec once implemented.
        info_every_n!(
            100,
            "Scraping gas prices starting from block {start_block_number} to {last_block_number}."
        );
        for block_number in start_block_number..=last_block_number {
            let header = match self.base_layer.get_block_header(block_number).await {
                Ok(Some(header)) => header,
                Ok(None) => return Ok(block_number),
                Err(e) => {
                    warn!("BaseLayerError during scraping: {e:?}");
                    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.increment(1);
                    return Ok(block_number);
                }
            };
            let timestamp = header.timestamp;
            let price_info = PriceInfo {
                base_fee_per_gas: GasPrice(header.base_fee_per_gas),
                blob_fee: GasPrice(header.blob_fee),
            };

            self.assert_no_l1_reorgs(&header).await?;
            // Save this block header to use for next iteration.
            self.last_l1_header = Some(header);

            self.l1_gas_price_provider
                .add_price_info(GasPriceData { block_number, timestamp, price_info })
                .await
                .map_err(L1GasPriceScraperError::GasPriceClientError)?;
            L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.increment(1);
        }
        Ok(last_block_number + 1)
    }
    async fn assert_no_l1_reorgs(
        &self,
        new_header: &L1BlockHeader,
    ) -> L1GasPriceScraperResult<(), B> {
        // If no last block was processed, we don't need to check for reorgs.
        let Some(ref last_header) = self.last_l1_header else {
            return Ok(());
        };

        if new_header.parent_hash != last_header.hash {
            L1_GAS_PRICE_SCRAPER_REORG_DETECTED.increment(1);
            return Err(L1GasPriceScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block hash, {}, for block number {}, is different from the \
                     hash stored, {}",
                    hex::encode(new_header.parent_hash),
                    last_header.number,
                    hex::encode(last_header.hash),
                ),
            });
        }

        Ok(())
    }

    async fn latest_l1_block_number(&self) -> L1GasPriceScraperResult<Option<L1BlockNumber>, B> {
        self.base_layer
            .latest_l1_block_number(self.config.finality)
            .await
            .map_err(L1GasPriceScraperError::BaseLayerError)
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync + Debug> ComponentStarter for L1GasPriceScraper<B>
where
    B::Error: Send,
{
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        register_scraper_metrics();
        let start_from = match self.config.starting_block {
            Some(block) => block,
            None => {
                let latest = self
                    .latest_l1_block_number()
                    .await
                    .expect("Failed to get the latest L1 block number at startup")
                    .expect("Failed to get the latest L1 block number at startup");

                // If no starting block is provided, the default is to start from
                // startup_num_blocks_multiplier * number_of_blocks_for_mean before the tip of L1.
                // Note that for new chains this subtraction may be negative,
                // hence the use of saturating_sub.
                latest.saturating_sub(
                    self.config.number_of_blocks_for_mean
                        * self.config.startup_num_blocks_multiplier,
                )
            }
        };
        self.run(start_from).await.unwrap_or_else(|e| panic!("L1 gas price scraper failed: {}", e))
    }
}
