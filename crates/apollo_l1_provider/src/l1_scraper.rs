use std::any::type_name;
use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::deserialize_float_seconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::validators::validate_ascii;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_l1_provider_types::errors::L1ProviderClientError;
use apollo_l1_provider_types::{Event, SharedL1ProviderClient};
use apollo_sequencer_infra::component_client::ClientError;
use apollo_sequencer_infra::component_definitions::ComponentStarter;
use async_trait::async_trait;
use papyrus_base_layer::constants::EventIdentifier;
use papyrus_base_layer::{BaseLayerContract, L1BlockNumber, L1BlockReference, L1Event};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::StarknetApiError;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{error, info, instrument};
use validator::Validate;

#[cfg(test)]
#[path = "l1_scraper_tests.rs"]
pub mod l1_scraper_tests;

type L1ScraperResult<T, B> = Result<T, L1ScraperError<B>>;

// Sensible lower bound.
const L1_BLOCK_TIME: u64 = 10;

pub struct L1Scraper<B: BaseLayerContract> {
    pub config: L1ScraperConfig,
    pub base_layer: B,
    pub last_l1_block_processed: L1BlockReference,
    pub l1_provider_client: SharedL1ProviderClient,
    tracked_event_identifiers: Vec<EventIdentifier>,
}

impl<B: BaseLayerContract + Send + Sync> L1Scraper<B> {
    pub async fn new(
        config: L1ScraperConfig,
        l1_provider_client: SharedL1ProviderClient,
        base_layer: B,
        events_identifiers_to_track: &[EventIdentifier],
    ) -> L1ScraperResult<Self, B> {
        let latest_l1_block = get_latest_l1_block_number(config.finality, &base_layer).await?;
        // Estimate the number of blocks in the interval, to rewind from the latest block.
        let blocks_in_interval = config.startup_rewind_time.as_secs() / L1_BLOCK_TIME;
        // Add 50% safety margin.
        let safe_blocks_in_interval = blocks_in_interval + blocks_in_interval / 2;

        let l1_block_number_rewind = latest_l1_block.saturating_sub(safe_blocks_in_interval);

        let block_reference_rewind = base_layer
            .l1_block_at(l1_block_number_rewind)
            .await
            .map_err(L1ScraperError::BaseLayerError)?
            .expect(
                "Rewound L1 block number is between 0 and the verified latest L1 block, so should \
                 exist",
            );

        Ok(Self {
            l1_provider_client,
            base_layer,
            last_l1_block_processed: block_reference_rewind,
            config,
            tracked_event_identifiers: events_identifiers_to_track.to_vec(),
        })
    }

    #[instrument(skip(self), err)]
    pub async fn initialize(&mut self) -> L1ScraperResult<(), B> {
        let (latest_l1_block, events) = self.fetch_events().await?;

        // If this gets too high, send in batches.
        let initialize_result = self.l1_provider_client.initialize(events).await;
        handle_client_error(initialize_result)?;

        self.last_l1_block_processed = latest_l1_block;

        Ok(())
    }

    pub async fn send_events_to_l1_provider(&mut self) -> L1ScraperResult<(), B> {
        self.assert_no_l1_reorgs().await?;

        let (latest_l1_block, events) = self.fetch_events().await?;

        // Sending even if there are no events, to keep the flow as simple/debuggable as possible.
        // Perf hit is minimal, since the scraper is on the same machine as the provider (no net).
        // If this gets spammy, short-circuit on events.empty().
        let add_events_result = self.l1_provider_client.add_events(events).await;
        handle_client_error(add_events_result)?;

        self.last_l1_block_processed = latest_l1_block;

        Ok(())
    }

    async fn fetch_events(&self) -> L1ScraperResult<(L1BlockReference, Vec<Event>), B> {
        let latest_l1_block = self
            .base_layer
            .latest_l1_block(self.config.finality)
            .await
            .map_err(L1ScraperError::BaseLayerError)?;

        let Some(latest_l1_block) = latest_l1_block else {
            return Err(
                L1ScraperError::finality_too_high(self.config.finality, &self.base_layer).await
            );
        };

        let scraping_start_number = self.last_l1_block_processed.number + 1;
        let scraping_result = self
            .base_layer
            .events(scraping_start_number..=latest_l1_block.number, &self.tracked_event_identifiers)
            .await;

        let events = scraping_result.map_err(L1ScraperError::BaseLayerError)?;
        let events = events
            .into_iter()
            .map(|event| self.event_from_raw_l1_event(event))
            .collect::<L1ScraperResult<Vec<_>, _>>()?;

        Ok((latest_l1_block, events))
    }

    #[instrument(skip(self), err)]
    async fn run(&mut self) -> L1ScraperResult<(), B> {
        self.initialize().await?;
        loop {
            sleep(self.config.polling_interval).await;

            self.send_events_to_l1_provider().await?;
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

    async fn assert_no_l1_reorgs(&self) -> L1ScraperResult<(), B> {
        let last_processed_l1_block_number = self.last_l1_block_processed.number;
        let last_block_processed_fresh = self
            .base_layer
            .l1_block_at(last_processed_l1_block_number)
            .await
            .map_err(L1ScraperError::BaseLayerError)?;

        let Some(last_block_processed_fresh) = last_block_processed_fresh else {
            return Err(L1ScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block with number {last_processed_l1_block_number} no \
                     longer exists."
                ),
            });
        };

        if last_block_processed_fresh.hash != self.last_l1_block_processed.hash {
            return Err(L1ScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block hash, {}, for block number {}, is different from the \
                     hash stored, {}",
                    hex::encode(last_block_processed_fresh.hash),
                    last_processed_l1_block_number,
                    hex::encode(self.last_l1_block_processed.hash),
                ),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> ComponentStarter for L1Scraper<B> {
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Runtime Error: {e}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ScraperConfig {
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub startup_rewind_time: Duration,
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub polling_interval: Duration,
}

impl Default for L1ScraperConfig {
    fn default() -> Self {
        Self {
            startup_rewind_time: Duration::from_secs(0),
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
                "startup_rewind_time",
                &self.startup_rewind_time.as_secs(),
                "Duration to rewind from latest L1 block when starting scraping.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "finality",
                &self.finality,
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
    #[error("Finality too high: {finality:?} > {latest_l1_block_no_finality:?}")]
    FinalityTooHigh { finality: u64, latest_l1_block_no_finality: L1BlockNumber },
    #[error("Failed to calculate hash: {0}")]
    HashCalculationError(StarknetApiError),
    // Leaky abstraction, these errors should not propagate here.
    #[error(transparent)]
    NetworkError(ClientError),
    #[error("L1 reorg detected: {reason}. Restart both the L1 provider and the scraper.")]
    L1ReorgDetected { reason: String },
}

impl<B: BaseLayerContract + Send + Sync> L1ScraperError<B> {
    pub async fn finality_too_high(finality: u64, base_layer: &B) -> L1ScraperError<B> {
        let latest_l1_block_number_no_finality = base_layer.latest_l1_block_number(0).await;
        let latest_l1_block_no_finality = match latest_l1_block_number_no_finality {
            Ok(block_number) => block_number
                .expect("Latest *L1* block without finality is assumed to always exist."),
            Err(error) => return Self::BaseLayerError(error),
        };

        Self::FinalityTooHigh { finality, latest_l1_block_no_finality }
    }
}

fn handle_client_error<B: BaseLayerContract + Send + Sync>(
    client_result: Result<(), L1ProviderClientError>,
) -> Result<(), L1ScraperError<B>> {
    if let Err(L1ProviderClientError::ClientError(client_error)) = client_result {
        return Err(L1ScraperError::NetworkError(client_error));
    }
    Ok(())
}

async fn get_latest_l1_block_number<B: BaseLayerContract + Send + Sync>(
    finality: u64,
    base_layer: &B,
) -> Result<L1BlockNumber, L1ScraperError<B>> {
    let latest_l1_block_number = base_layer
        .latest_l1_block_number(finality)
        .await
        .map_err(L1ScraperError::BaseLayerError)?;

    match latest_l1_block_number {
        Some(latest_l1_block_number) => Ok(latest_l1_block_number),
        None => Err(L1ScraperError::finality_too_high(finality, base_layer).await),
    }
}
