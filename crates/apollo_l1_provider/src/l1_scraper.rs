use std::any::type_name;
use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::{debug_every_n, info_every_n_sec};
use apollo_l1_provider_types::errors::{L1ProviderClientError, L1ProviderError};
use apollo_l1_provider_types::{Event, SharedL1ProviderClient};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_time::time::{Clock, DefaultClock};
use async_trait::async_trait;
use itertools::zip_eq;
use papyrus_base_layer::constants::EventIdentifier;
use papyrus_base_layer::{BaseLayerContract, L1BlockNumber, L1BlockReference, L1Event};
use starknet_api::block::BlockNumber;
use starknet_api::StarknetApiError;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, trace, warn};

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

// TODO(guyn): make this a config parameter
// Sensible lower bound.
const L1_BLOCK_TIME: u64 = 10;

pub struct L1Scraper<B: BaseLayerContract> {
    pub config: L1ScraperConfig,
    pub base_layer: B,
    pub scrape_from_this_l1_block: Option<L1BlockReference>,
    pub l1_provider_client: SharedL1ProviderClient,
    tracked_event_identifiers: Vec<EventIdentifier>,
    pub clock: Arc<dyn Clock>,
}

impl<B: BaseLayerContract + Send + Sync> L1Scraper<B> {
    pub async fn new(
        config: L1ScraperConfig,
        l1_provider_client: SharedL1ProviderClient,
        base_layer: B,
        events_identifiers_to_track: &[EventIdentifier],
    ) -> L1ScraperResult<Self, B> {
        Ok(Self {
            l1_provider_client,
            base_layer,
            scrape_from_this_l1_block: None,
            config,
            tracked_event_identifiers: events_identifiers_to_track.to_vec(),
            clock: Arc::new(DefaultClock),
        })
    }

    /// Use config.startup_rewind_time_seconds to estimate an L1 block number
    /// that is far enough back to start scraping from.
    pub async fn fetch_start_block(&self) -> Result<L1BlockReference, L1ScraperError<B>> {
        let finality = self.config.finality;
        let latest_l1_block_number = self
            .base_layer
            .latest_l1_block_number(finality)
            .await
            .map_err(L1ScraperError::BaseLayerError)?;
        debug!("Latest L1 block number: {latest_l1_block_number:?}");

        // Estimate the number of blocks in the interval, to rewind from the latest block.
        let blocks_in_interval = self.config.startup_rewind_time_seconds.as_secs() / L1_BLOCK_TIME;
        debug!("Blocks in interval: {blocks_in_interval}");

        // Add 50% safety margin.
        let safe_blocks_in_interval = blocks_in_interval + blocks_in_interval / 2;
        debug!("Safe blocks in interval: {safe_blocks_in_interval}");

        let l1_block_number_rewind = latest_l1_block_number.saturating_sub(safe_blocks_in_interval);
        debug!("L1 block number rewind: {l1_block_number_rewind}");

        let block_reference_rewind = self
            .base_layer
            .l1_block_at(l1_block_number_rewind)
            .await
            .map_err(L1ScraperError::BaseLayerError)?
            .unwrap_or_else(|| {
                panic!(
                    "Rewound L1 block number is between 0 and the verified latest L1 block \
                     {latest_l1_block_number}, so should exist",
                )
            });
        debug!("Block reference rewind: {block_reference_rewind:?}");

        Ok(block_reference_rewind)
    }

    /// Get the last historic L2 height that was proved before the start block number.
    async fn get_last_historic_l2_height(&self) -> L1ScraperResult<BlockNumber, B> {
        let Some(start_block) = self.scrape_from_this_l1_block else {
            panic!(
                "Should never get last historic L2 height without first getting the last \
                 processed L1 block."
            );
        };
        let last_historic_l2_height = self
            .base_layer
            .get_proved_block_at(start_block.number)
            .await
            .map_err(L1ScraperError::BaseLayerError)?
            .number;
        Ok(last_historic_l2_height)
    }

    /// Send an initialize message to the L1 provider, including the last L2 height that was proved
    /// before the start block number, and all events scraped from that L1 block until current
    /// block. The provider will return an error if it was already initialized (e.g., if the scraper
    /// was restarted).
    #[instrument(skip(self), err)]
    async fn initialize(&mut self, last_historic_l2_height: BlockNumber) -> L1ScraperResult<(), B> {
        let (latest_l1_block, events) = self.fetch_events().await?;

        debug!("Latest L1 block for initialize: {latest_l1_block:?}");
        debug!("All events scraped during initialize: {events:?}");

        // If this gets too high, send in batches.
        let initialize_result =
            self.l1_provider_client.initialize(last_historic_l2_height, events).await;
        handle_client_error(initialize_result)?;

        // Successfully scraped events up to latest l1 block.
        self.scrape_from_this_l1_block = Some(latest_l1_block);

        Ok(())
    }

    /// Scrape recent events and send them to the L1 provider.
    pub async fn send_events_to_l1_provider(&mut self) -> L1ScraperResult<(), B> {
        self.assert_no_l1_reorgs().await?;

        let (latest_l1_block, events) = self.fetch_events().await?;
        // TODO(guyn): remove these _every_n_sec because the polling interval is longer.
        trace!("scraped up to {latest_l1_block:?}");
        info_every_n_sec!(1, "scraped up to {latest_l1_block:?}");

        // Sending even if there are no events, to keep the flow as simple/debuggable as possible.
        // Perf hit is minimal, since the scraper is on the same machine as the provider (no
        // network). If this gets spammy, short-circuit on events.empty().
        let add_events_result = self.l1_provider_client.add_events(events).await;
        handle_client_error(add_events_result)?;

        // Successfully scraped events up to latest l1 block.
        self.scrape_from_this_l1_block = Some(latest_l1_block);

        Ok(())
    }

    /// Query the L1 base layer for all events since scrape_from_this_l1_block.
    async fn fetch_events(&self) -> L1ScraperResult<(L1BlockReference, Vec<Event>), B> {
        let scrape_timestamp = self.clock.unix_now();

        let latest_l1_block = self
            .base_layer
            .latest_l1_block(self.config.finality)
            .await
            .map_err(L1ScraperError::BaseLayerError)?;

        let Some(latest_l1_block) = latest_l1_block else {
            // TODO(guyn): get rid of finality_too_high, use a better error.
            return Err(
                L1ScraperError::finality_too_high(self.config.finality, &self.base_layer).await
            );
        };
        let Some(scrape_from_this_l1_block) = self.scrape_from_this_l1_block else {
            panic!("Should never fetch events without first getting the last processed L1 block.");
        };
        // This can happen if, e.g., changing base layers. Should ignore and try scraping again.
        if latest_l1_block.number <= scrape_from_this_l1_block.number {
            warn!(
                "Latest L1 block number {} is not greater than the last processed L1 block number \
                 {}. Ignoring, will try again on the next interval.",
                latest_l1_block.number, scrape_from_this_l1_block.number
            );
            return Ok((scrape_from_this_l1_block, vec![]));
        }

        let scraping_start_number = scrape_from_this_l1_block.number + 1;
        let scraping_result = self
            .base_layer
            .events(scraping_start_number..=latest_l1_block.number, &self.tracked_event_identifiers)
            .await;

        let l1_events = scraping_result.map_err(L1ScraperError::BaseLayerError)?;
        // Used for debug. Collect the L1 tx hashes and L1 block timestamps.
        let l1_messages_info = l1_events
            .iter()
            .filter_map(|event| match event {
                L1Event::LogMessageToL2 { l1_tx_hash, block_timestamp, .. } => {
                    Some((*l1_tx_hash, *block_timestamp))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        // Convert L1 events into Starknet provider events. Includes calculating the L2 tx hash.
        let events = l1_events
            .into_iter()
            .map(|event| {
                Event::from_l1_event(&self.config.chain_id, event, scrape_timestamp)
                    .map_err(L1ScraperError::HashCalculationError)
            })
            .collect::<L1ScraperResult<Vec<_>, _>>()?;

        // Used for debug. Collect the L2 hashes for events that are L1 handler transactions.
        let l2_hashes = events.iter().filter_map(|event| match event {
            Event::L1HandlerTransaction { l1_handler_tx, .. } => Some(l1_handler_tx.tx_hash),
            _ => None,
        });

        let formatted_pairs = zip_eq(l1_messages_info, l2_hashes)
            .map(|((l1_hash, timestamp), l2_hash)| {
                format!("L1 tx hash: {l1_hash:?}, L1 timestamp: {timestamp}, L2 tx hash: {l2_hash}")
            })
            .collect::<Vec<_>>();
        if formatted_pairs.is_empty() {
            debug_every_n!(100, "Got Messages to L2: []");
        } else {
            debug!("Got Messages to L2: {formatted_pairs:?}");
        }

        // Debug: log cancellation started events.
        let cancellation_started_events = events
            .iter()
            .filter_map(|event| match event {
                Event::TransactionCancellationStarted { tx_hash, .. } => Some(*tx_hash),
                _ => None,
            })
            .collect::<Vec<_>>();
        let formatted_cancellation_started_events = cancellation_started_events
            .iter()
            .map(|tx_hash| format!("Cancel tx with L2 hash: {tx_hash}"));
        if cancellation_started_events.is_empty() {
            debug_every_n!(100, "Got Cancellation Started Events: []");
        } else {
            debug!("Got Cancellation Started Events: {formatted_cancellation_started_events:?}");
        }
        Ok((latest_l1_block, events))
    }

    async fn sleep_for_base_layer_error(
        &self,
        description: &str,
        e: L1ScraperError<B>,
    ) -> L1ScraperResult<(), B> {
        match &e {
            L1ScraperError::BaseLayerError(_) => {
                warn!("Error while {description}: {e}");
                L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.increment(1);
                // TODO(guyn): do we want a different interval here? Maybe doesn't really matter.
                sleep(self.config.polling_interval_seconds).await;
                Ok(())
            }
            _ => Err(e),
        }
    }

    #[instrument(skip(self), err)]
    pub async fn run(&mut self) -> L1ScraperResult<(), B> {
        // This is the startup loop, getting start blocks (on L1 and L2) and sending the first batch
        // of scraped events to the provider.
        loop {
            match self.fetch_start_block().await {
                Ok(start_block) => {
                    // We didn't scrape anything up to this block, but it is far enough in the past
                    // that we can ignore events up to this block (inclusive).
                    self.scrape_from_this_l1_block = Some(start_block);
                    debug!("Start block on L1 is: {start_block:?}");
                }
                Err(e) => {
                    self.sleep_for_base_layer_error("fetching start block", e).await?;
                    continue;
                }
            }

            // TODO(guyn): add an override in case the chain does not have any proved blocks yet.
            let last_historic_l2_height = match self.get_last_historic_l2_height().await {
                Ok(height) => {
                    debug!("Last historic L2 height is: {height}");
                    height
                }
                Err(e) => {
                    self.sleep_for_base_layer_error("fetching last historic L2 height", e).await?;
                    continue;
                }
            };

            // Initialize fetches events from start block up to latest, sends them to the provider.
            // TODO(guyn): the next comment will be implemented in one of the next PRs:
            // It also sends the provider the start l1 block number, to allow it to find the l2
            // block height to start sync from.
            match self.initialize(last_historic_l2_height).await {
                Err(e) => {
                    self.sleep_for_base_layer_error("initializing", e).await?;
                    continue;
                }
                Ok(_) => break,
            };
        }

        // This is the main (steady state) loop.
        loop {
            // Sleep at start of loop, as we get here right after successful initialize+break.
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

    /// Fetch scrape_from_this_l1_block again, check it still exists and that its hash is the same.
    /// If a reorg occurred up to this block, return an error (existing data in the provider is
    /// stale).
    async fn assert_no_l1_reorgs(&self) -> L1ScraperResult<(), B> {
        let Some(scrape_from_this_l1_block) = self.scrape_from_this_l1_block else {
            panic!(
                "Should never assert no l1 reorgs without first getting the last processed L1 \
                 block."
            );
        };
        let last_processed_l1_block_number = scrape_from_this_l1_block.number;
        let last_processed_l1_block_hash = scrape_from_this_l1_block.hash;
        let last_block_processed_fresh = self
            .base_layer
            .l1_block_at(last_processed_l1_block_number)
            .await
            .map_err(L1ScraperError::BaseLayerError)?;

        let Some(last_block_processed_fresh) = last_block_processed_fresh else {
            L1_MESSAGE_SCRAPER_REORG_DETECTED.increment(1);
            return Err(L1ScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block with number {last_processed_l1_block_number} and \
                     hash {last_processed_l1_block_hash} no longer exists."
                ),
            });
        };

        if last_block_processed_fresh.hash != last_processed_l1_block_hash {
            L1_MESSAGE_SCRAPER_REORG_DETECTED.increment(1);
            return Err(L1ScraperError::L1ReorgDetected {
                reason: format!(
                    "Last processed L1 block hash, {}, for block number {}, is different from the \
                     hash stored, {}",
                    last_block_processed_fresh.hash,
                    last_processed_l1_block_number,
                    last_processed_l1_block_hash
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
        register_scraper_metrics();
        self.run().await.unwrap_or_else(|e| panic!("Runtime Error: {e}"))
    }
}

#[derive(Error, Debug)]
pub enum L1ScraperError<T: BaseLayerContract + Send + Sync> {
    #[error("Base layer error: {0}")]
    BaseLayerError(T::Error),
    #[error(
        "Could not find block number. Finality {finality:?}, latest block: \
         {latest_l1_block_no_finality:?}"
    )]
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

// TODO(guyn): get rid of finality_too_high, use a better error.
impl<B: BaseLayerContract + Send + Sync> L1ScraperError<B> {
    /// Pass any base layer errors. In the rare case that the finality is bigger than the latest L1
    /// block number, return FinalityTooHigh.
    pub async fn finality_too_high(finality: u64, base_layer: &B) -> L1ScraperError<B> {
        let latest_l1_block_number_no_finality = base_layer.latest_l1_block_number(0).await;

        let latest_l1_block_no_finality = match latest_l1_block_number_no_finality {
            Ok(block_number) => block_number,
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
