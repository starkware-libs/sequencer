use std::time::Duration;

use papyrus_base_layer::constants::EventIdentifier;
use papyrus_base_layer::{BaseLayerContract, L1Event};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::StarknetApiError;
use starknet_l1_provider_types::{Event, SharedL1ProviderClient};
use thiserror::Error;
use tokio::time::sleep;
use tracing::error;
use validator::Validate;

type L1ScraperResult<T, B> = Result<T, L1ScraperError<B>>;

pub struct L1Scraper<B: BaseLayerContract> {
    pub config: L1ScraperConfig,
    pub base_layer: B,
    pub last_block_number_processed: u64,
    pub l1_provider_client: SharedL1ProviderClient,
    tracked_event_identifiers: Vec<EventIdentifier>,
}

impl<B: BaseLayerContract + Send + Sync> L1Scraper<B> {
    pub fn new(
        config: L1ScraperConfig,
        l1_provider_client: SharedL1ProviderClient,
        base_layer: B,
        events_identifiers_to_track: &[EventIdentifier],
    ) -> Self {
        Self {
            l1_provider_client,
            base_layer,
            last_block_number_processed: config.l1_block_to_start_scraping_from,
            config,
            tracked_event_identifiers: events_identifiers_to_track.to_vec(),
        }
    }

    pub async fn fetch_events(&mut self) -> L1ScraperResult<(), B> {
        let latest_l1_block_number = self
            .base_layer
            .latest_l1_block_number(self.config.finality)
            .await
            .map_err(L1ScraperError::BaseLayer)?;

        let Some(latest_l1_block_number) = latest_l1_block_number else {
            error!("Failed to get latest L1 block number, finality too high.");
            return Ok(());
        };

        let scraping_result = self
            .base_layer
            .events(
                self.last_block_number_processed..=latest_l1_block_number,
                &self.tracked_event_identifiers,
            )
            .await;

        let events = scraping_result.map_err(L1ScraperError::BaseLayer)?;
        let events = events
            .into_iter()
            .map(|event| self.event_from_raw_l1_event(event))
            .collect::<L1ScraperResult<Vec<_>, _>>()?;

        self.last_block_number_processed = latest_l1_block_number + 1;
        todo!("send {events:?} to provider");
    }

    async fn _run(&mut self) -> L1ScraperResult<(), B> {
        loop {
            sleep(self.config.polling_interval).await;
            // TODO: retry.
            self.fetch_events().await?;
        }
    }

    fn event_from_raw_l1_event(&self, l1_event: L1Event) -> L1ScraperResult<Event, B> {
        match l1_event {
            L1Event::LogMessageToL2 { tx, fee } => {
                let chain_id = &self.config.chain_id;
                ExecutableL1HandlerTransaction::create(tx, chain_id, fee)
                    .map(Event::L1HandlerTransaction)
                    .map_err(L1ScraperError::HashCalculationError)
            }
            L1Event::MessageToL2CancellationStarted(_messsage_data) => todo!(),
            L1Event::MessageToL2Canceled(_messsage_data) => todo!(),
            L1Event::ConsumedMessageToL2(_messsage_data) => todo!(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ScraperConfig {
    pub l1_block_to_start_scraping_from: u64,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub polling_interval: Duration,
}

impl Default for L1ScraperConfig {
    fn default() -> Self {
        Self {
            l1_block_to_start_scraping_from: 0,
            chain_id: ChainId::Mainnet,
            finality: 0,
            polling_interval: Duration::from_secs(1),
        }
    }
}

#[derive(Error, Debug)]
pub enum L1ScraperError<T: BaseLayerContract + Send + Sync> {
    #[error("Base layer error: {0}")]
    BaseLayer(T::Error),
    #[error("Failed to calculate hash: {0}")]
    HashCalculationError(StarknetApiError),
}
