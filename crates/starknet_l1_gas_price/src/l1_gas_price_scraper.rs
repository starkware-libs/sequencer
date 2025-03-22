use std::any::type_name;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_base_layer::{BaseLayerContract, L1BlockNumber};
use papyrus_config::converters::deserialize_float_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::validators::validate_ascii;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_l1_gas_price_types::errors::L1GasPriceClientError;
use starknet_l1_gas_price_types::L1GasPriceProviderClient;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use thiserror::Error;
use tracing::{error, info};
use validator::Validate;

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
}

impl Default for L1GasPriceScraperConfig {
    fn default() -> Self {
        Self {
            starting_block: None,
            chain_id: ChainId::Other("0x0".to_string()),
            finality: 0,
            polling_interval: Duration::from_secs(1),
            number_of_blocks_for_mean: 300,
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
}

impl<B: BaseLayerContract + Send + Sync> L1GasPriceScraper<B> {
    pub fn new(
        config: L1GasPriceScraperConfig,
        l1_gas_price_provider: SharedL1GasPriceProvider,
        base_layer: B,
    ) -> Self {
        Self { config, l1_gas_price_provider, base_layer }
    }

    /// Run the scraper, starting from the given L1 `block_num`, indefinitely.
    async fn run(&mut self, mut block_num: L1BlockNumber) -> L1GasPriceScraperResult<(), B> {
        loop {
            block_num = self.update_prices(block_num).await?;
            tokio::time::sleep(self.config.polling_interval).await;
        }
    }

    /// Scrape all blocks the provider knows starting from `block_num`.
    /// Returns the next `block_num` to be scraped.
    async fn update_prices(
        &mut self,
        mut block_num: L1BlockNumber,
    ) -> L1GasPriceScraperResult<L1BlockNumber, B> {
        while let Some(sample) = self
            .base_layer
            .get_price_sample(block_num)
            .await
            .map_err(L1GasPriceScraperError::BaseLayerError)?
        {
            self.l1_gas_price_provider
                .add_price_info(block_num, sample)
                .await
                .map_err(L1GasPriceScraperError::GasPriceClientError)?;

            block_num += 1;
        }
        Ok(block_num)
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> ComponentStarter for L1GasPriceScraper<B>
where
    B::Error: Send,
{
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        println!("\n\nself.config.finality: {}\n\n", self.config.finality);
        let start_from = match self.config.starting_block {
            Some(block) => block,
            None => {
                let latest = self
                    .base_layer
                    .latest_l1_block_number(self.config.finality)
                    .await
                    .expect("Failed to get the latest L1 block number")
                    .expect("Failed to get the latest L1 block number");
                // If no starting block is provided, the default is to start from
                // 2 * number_of_blocks_for_mean before the tip of L1.
                // Note that for new chains this subtraction may be negative,
                // hence the use of saturating_sub.
                latest.saturating_sub(self.config.number_of_blocks_for_mean * 2)
            }
        };
        self.run(start_from)
            .await
            .unwrap_or_else(|e| panic!("Failed to start L1Scraper component: {}", e))
    }
}
