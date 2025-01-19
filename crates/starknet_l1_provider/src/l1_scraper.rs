use std::any::type_name;
use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_base_layer::constants::EventIdentifier;
use papyrus_base_layer::{BaseLayerContract, L1Event};
use papyrus_config::converters::deserialize_float_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::validators::validate_ascii;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::StarknetApiError;
use starknet_l1_provider_types::errors::L1ProviderClientError;
use starknet_l1_provider_types::{Event, SharedL1ProviderClient};
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{error, info};
use validator::Validate;

type L1ScraperResult<T, B> = Result<T, L1ScraperError<B>>;

#[cfg(test)]
#[path = "l1_scraper_tests.rs"]
pub mod l1_scraper_tests;

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
            .map_err(L1ScraperError::BaseLayerError)?;

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

        let events = scraping_result.map_err(L1ScraperError::BaseLayerError)?;
        let events = events
            .into_iter()
            .map(|event| self.event_from_raw_l1_event(event))
            .collect::<L1ScraperResult<Vec<_>, _>>()?;

        if let Err(L1ProviderClientError::ClientError(client_error)) =
            self.l1_provider_client.add_events(events).await
        {
            return Err(L1ScraperError::NetworkError(client_error));
        }
        self.last_block_number_processed = latest_l1_block_number + 1;
        Ok(())
    }

    async fn run(&mut self) -> L1ScraperResult<(), B> {
        loop {
            sleep(self.config.polling_interval).await;
            // FIXME: Configure Anvil for integration tests later, currently this fails there
            // because anvil isn't configured.
            let _error_in_flow_tests = self.fetch_events().await;
        }
    }

    fn event_from_raw_l1_event(&self, l1_event: L1Event) -> L1ScraperResult<Event, B> {
        match l1_event {
            L1Event::LogMessageToL2 { tx, fee } => {
                let chain_id = &self.config.chain_id;
                match ExecutableL1HandlerTransaction::create(tx, chain_id, fee) {
                    Ok(tx) => Ok(Event::L1HandlerTransaction(tx)),
                    Err(hash_calc_err) => Err(L1ScraperError::HashCalculationError(hash_calc_err)),
                }
            }
            L1Event::MessageToL2CancellationStarted(_messsage_data) => todo!(),
            L1Event::MessageToL2Canceled(_messsage_data) => todo!(),
            L1Event::ConsumedMessageToL2(_messsage_data) => todo!(),
        }
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> ComponentStarter for L1Scraper<B> {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|_| ComponentError::InternalComponentError)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ScraperConfig {
    pub l1_block_to_start_scraping_from: u64,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
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

impl SerializeConfig for L1ScraperConfig {
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

#[derive(Error, Debug)]
pub enum L1ScraperError<T: BaseLayerContract + Send + Sync> {
    #[error("Base layer error: {0}")]
    BaseLayerError(T::Error),
    #[error("Failed to calculate hash: {0}")]
    HashCalculationError(StarknetApiError),
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
}
