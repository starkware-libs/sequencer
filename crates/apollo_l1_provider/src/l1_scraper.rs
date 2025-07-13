use std::any::type_name;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::{debug_every_n, info_every_n_sec};
use apollo_l1_provider_types::errors::{L1ProviderClientError, L1ProviderError};
use apollo_l1_provider_types::{Event, SharedL1ProviderClient};
use async_trait::async_trait;
use itertools::zip_eq;
use papyrus_base_layer::constants::EventIdentifier;
use papyrus_base_layer::{BaseLayerContract, L1BlockNumber, L1BlockReference, L1Event};
use starknet_api::StarknetApiError;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, info, instrument, trace, warn};

use crate::config::L1ScraperConfig;
use crate::metrics::{
    register_scraper_metrics,
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};

#[cfg(test)]
#[path = "l1_scraper_tests.rs"]
pub mod l1_scraper_tests;

type L1ScraperResult<T, B> = Result<T, L1ScraperError<B>>;

// Sensible lower bound.
pub const L1_BLOCK_TIME: u64 = 10;

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
        l1_start_block: L1BlockReference,
    ) -> L1ScraperResult<Self, B> {
        Ok(Self {
            l1_provider_client,
            base_layer,
            last_l1_block_processed: l1_start_block,
            config,
            tracked_event_identifiers: events_identifiers_to_track.to_vec(),
        })
    }

    #[instrument(skip(self), err)]
    async fn initialize(&mut self) -> L1ScraperResult<(), B> {
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
        trace!("scraped up to {latest_l1_block:?}");
        info_every_n_sec!(1, "scraped up to {latest_l1_block:?}");

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

        let l1_events = scraping_result.map_err(L1ScraperError::BaseLayerError)?;
        // Used for debug.
        let l1_messages_info = l1_events
            .iter()
            .filter_map(|event| match event {
                L1Event::LogMessageToL2 { l1_tx_hash, timestamp, .. } => {
                    Some((*l1_tx_hash, *timestamp))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let events = l1_events
            .into_iter()
            .map(|event| {
                Event::from_l1_event(&self.config.chain_id, event)
                    .map_err(L1ScraperError::HashCalculationError)
            })
            .collect::<L1ScraperResult<Vec<_>, _>>()?;

        // Used for debug.
        let l2_hashes = events.iter().filter_map(|event| match event {
            Event::L1HandlerTransaction { l1_handler_tx, .. } => Some(l1_handler_tx.tx_hash),
            _ => None,
        });

        let formatted_pairs = zip_eq(l1_messages_info, l2_hashes)
            .map(|((l1_hash, timestamp), l2_hash)| {
                format!("L1 hash: {l1_hash:?}, L1 timestamp: {timestamp}, L2 hash: {l2_hash}")
            })
            .collect::<Vec<_>>();
        if formatted_pairs.is_empty() {
            debug_every_n!(100, "Got Messages to L2: []");
        } else {
            debug!("Got Messages to L2: {formatted_pairs:?}");
        }
        Ok((latest_l1_block, events))
    }

    #[instrument(skip(self), err)]
    pub async fn run(&mut self) -> L1ScraperResult<(), B> {
        loop {
            match self.initialize().await {
                Err(L1ScraperError::BaseLayerError(e)) => {
                    warn!("BaseLayerError during initialization: {e:?}");
                    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.increment(1);
                }
                Ok(_) => break,
                Err(e) => return Err(e),
            };

            // Outside of the match branch due to lifetime issues.
            sleep(self.config.polling_interval_seconds).await;
        }

        loop {
            sleep(self.config.polling_interval_seconds).await;

            match self.send_events_to_l1_provider().await {
                Err(L1ScraperError::BaseLayerError(e)) => {
                    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.increment(1);
                    warn!("BaseLayerError during scraping: {e:?}");
                }
                Ok(_) => {
                    L1_MESSAGE_SCRAPER_SUCCESS_COUNT.increment(1);
                }
                Err(e) => return Err(e),
            }
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
            L1_MESSAGE_SCRAPER_REORG_DETECTED.increment(1);
            return Err(L1ScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block with number {last_processed_l1_block_number} no \
                     longer exists."
                ),
            });
        };

        if last_block_processed_fresh.hash != self.last_l1_block_processed.hash {
            L1_MESSAGE_SCRAPER_REORG_DETECTED.increment(1);
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

pub async fn fetch_start_block<B: BaseLayerContract + Send + Sync>(
    base_layer: &B,
    config: &L1ScraperConfig,
) -> Result<L1BlockReference, L1ScraperError<B>> {
    let finality = config.finality;
    let latest_l1_block_number = base_layer
        .latest_l1_block_number(finality)
        .await
        .map_err(L1ScraperError::BaseLayerError)?;

    let latest_l1_block = match latest_l1_block_number {
        Some(latest_l1_block_number) => Ok(latest_l1_block_number),
        None => Err(L1ScraperError::finality_too_high(finality, base_layer).await),
    }?;

    // Estimate the number of blocks in the interval, to rewind from the latest block.
    let blocks_in_interval = config.startup_rewind_time_seconds.as_secs() / L1_BLOCK_TIME;
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
    Ok(block_reference_rewind)
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> ComponentStarter for L1Scraper<B> {
    async fn start(&mut self) {
        info!("Starting component {}.", type_name::<Self>());
        register_scraper_metrics();
        self.run().await.unwrap_or_else(|e| panic!("Runtime Error: {e}"))
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
    // This is likely due to a provider crash, which is now waiting for the restart sequence from
    // the scraper.
    #[error("The scraper requires a restart.")]
    NeedsRestart,
}

impl<B: BaseLayerContract + Send + Sync> PartialEq for L1ScraperError<B> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::BaseLayerError(e1), Self::BaseLayerError(e2)) => e1 == e2,
            (this @ Self::FinalityTooHigh { .. }, other @ Self::FinalityTooHigh { .. }) => {
                this == other
            }
            (Self::HashCalculationError(e1), Self::HashCalculationError(e2)) => e1 == e2,
            (Self::NetworkError(e1), Self::NetworkError(e2)) => e1 == e2,
            (this @ Self::L1ReorgDetected { .. }, other @ Self::L1ReorgDetected { .. }) => {
                this == other
            }
            (Self::NeedsRestart, Self::NeedsRestart) => true,
            _ => false,
        }
    }
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
    let Err(error) = client_result else {
        return Ok(());
    };
    match error {
        L1ProviderClientError::ClientError(client_error) => {
            Err(L1ScraperError::NetworkError(client_error))
        }
        L1ProviderClientError::L1ProviderError(L1ProviderError::Uninitialized) => {
            Err(L1ScraperError::NeedsRestart)
        }
        L1ProviderClientError::L1ProviderError(L1ProviderError::UnsupportedL1Event(event)) => {
            panic!(
                "Scraper-->Provider consistency error: the event {event} is not supported by the \
                 provider, but has been scraped and sent to it nonetheless. Check the list of \
                 tracked events in the scraper and compare to the provider's."
            )
        }
        error => panic!("Unexpected error: {error}"),
    }
}
