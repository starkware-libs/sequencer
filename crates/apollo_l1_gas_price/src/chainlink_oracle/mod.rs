//! Reading Chainlink's price feeds on Starknet: the oracle client consensus calls, and the feed
//! reads behind it.

use std::fmt::{Debug, Formatter, Result as FormatterResult};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_l1_gas_price_config::config::{AllRateBoundsConfig, ChainlinkOracleConfig};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{
    EthToFri,
    ExchangeRate,
    ExchangeRateOracleClientTrait,
    RateKind,
    StrkToUsd,
};
use async_trait::async_trait;
use futures::FutureExt;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, info, instrument, warn};

use crate::chainlink_oracle::feed_math::{derive_eth_to_fri_rate, RateResult};
use crate::chainlink_oracle::feed_read::{read_feed, ChainlinkFeeds};
use crate::metrics::{
    ExchangeRateOracleMetrics,
    ETH_TO_STRK_ORACLE_METRICS,
    STRK_TO_USD_ORACLE_METRICS,
};
use crate::rate_bounds::check_rate_bounds;

mod contract_call_error;
mod feed_decode;
pub(crate) mod feed_math;
mod feed_read;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_utils;

// How many sampling intervals may separate the last valid read's block timestamp from the block
// timestamp being served, while that read is still served. Three read intervals, 45 minutes at the
// production 900 second interval. The bound applies in both directions, because a re-proposal can
// ask for a timestamp earlier than the read the client holds.
const MAX_FALLBACK_SAMPLING_INTERVALS: u64 = 3;

// A read in flight, resolving to a rate already dated by the block timestamp it was issued for.
// A plain handle, not an abort-on-drop one: aborting mid-`call_contract` drops the response
// receiver while the batcher still holds the sender, which panics its local component server. The
// slot is never reused while occupied, so there is nothing for abort-on-drop to protect.
type RateQuery = JoinHandle<Result<ValidRead, ExchangeRateOracleClientError>>;

// A rate that passed every guard, and the block timestamp it was read for. That timestamp is a
// block timestamp rather than a local one, because the distance a later caller measures against it
// must come out the same on every node, including on a replay of the same block.
#[derive(Clone, Copy)]
struct ValidRead {
    rate: ExchangeRate,
    block_timestamp: u64,
}

/// One pair's oracle state. There is one instance per `RateKind`, including the derived ETH/STRK
/// pair, which caches and refreshes its combined rate independently of the legs it is built from.
#[derive(Default)]
struct PairOracleState {
    // The newest read that passed every guard, served to callers while it is within
    // `MAX_FALLBACK_SAMPLING_INTERVALS` of their own block timestamp.
    last_valid_read: Option<ValidRead>,
    // The newest query's failure, cleared by the next success. Served only when no valid read is
    // close enough to the caller.
    last_error: Option<ExchangeRateOracleClientError>,
    // The query in flight. A single slot bounds this client to one query at a time.
    query: Option<RateQuery>,
    // When the last query was spawned, on the local monotonic clock, which the refresh cadence is
    // measured from. Local because the cadence is this node's own scheduling, so a block timestamp
    // arriving from the network cannot steer it.
    last_attempt_instant: Option<Instant>,
}

/// The Chainlink read behind a `RateKind`. Separate from `RateKind` because
/// `ExchangeRateOracleMetrics` lives in this crate, which the types crate cannot name.
#[async_trait]
pub trait ChainlinkRate: RateKind {
    /// The pair's metrics bundle, shared with the HTTP client for the same pair so each keeps one
    /// set of Prometheus series across a migration between the two sources.
    fn metrics() -> ExchangeRateOracleMetrics;

    async fn query_rate(
        batcher_client: &SharedBatcherClient,
        config: &ChainlinkOracleConfig,
        bounds_config: &AllRateBoundsConfig,
        block_timestamp: u64,
    ) -> RateResult;
}

/// Reads Chainlink's on-chain Starknet price feeds through the sequencer's own batcher.
///
/// Consensus calls `fetch_rate` on every proposal build and validate, so the call must not block on
/// the batcher: the feed is read by a background query spawned at most once per
/// `sampling_interval_seconds`, and every caller is served the last valid read while that read is
/// within `MAX_FALLBACK_SAMPLING_INTERVALS` of the caller's own block timestamp.
///
/// Reads are not deterministic across nodes: `call_contract` executes against the batcher's latest
/// committed block rather than state pinned to the queried timestamp, so two nodes can read
/// different rounds for the same block timestamp. Chainlink's deviation threshold is far inside the
/// `l1_gas_price_margin_percent` validators compare within, so this is not expected to reject
/// proposals.
// [Temporary comment] Constructed only by tests; B4 builds it for a feed whose configured oracle
// source is Chainlink.
pub struct ChainlinkOracleClient<Kind: ChainlinkRate> {
    config: ChainlinkOracleConfig,
    bounds_config: AllRateBoundsConfig,
    batcher_client: SharedBatcherClient,
    state: Arc<Mutex<PairOracleState>>,
    metrics: ExchangeRateOracleMetrics,
    _kind: PhantomData<Kind>,
}

// Manual impl: the trait requires `Debug` but `SharedBatcherClient` does not provide it.
impl<Kind: ChainlinkRate> Debug for ChainlinkOracleClient<Kind> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatterResult {
        formatter
            .debug_struct("ChainlinkOracleClient")
            .field("pair", &Kind::PAIR)
            .field("config", &self.config)
            .field("bounds_config", &self.bounds_config)
            .finish_non_exhaustive()
    }
}

impl<Kind: ChainlinkRate> ChainlinkOracleClient<Kind> {
    pub fn new(
        config: ChainlinkOracleConfig,
        bounds_config: AllRateBoundsConfig,
        batcher_client: SharedBatcherClient,
    ) -> Self {
        let pair = Kind::PAIR;
        info!("Creating ChainlinkOracleClient for {pair:?} with: {config:?} {bounds_config:?}");
        let metrics = Kind::metrics();
        metrics.register();
        Self {
            config,
            bounds_config,
            batcher_client,
            state: Arc::new(Mutex::new(PairOracleState::default())),
            metrics,
            _kind: PhantomData,
        }
    }

    // `block_timestamp` is what every freshness guard inside the query is measured against, and
    // what the resulting read is dated by.
    fn spawn_query(&self, block_timestamp: u64) -> RateQuery {
        let batcher_client = self.batcher_client.clone();
        let config = self.config.clone();
        let bounds_config = self.bounds_config.clone();
        let metrics = self.metrics;
        let pair = Kind::PAIR;
        tokio::spawn(async move {
            let result =
                Kind::query_rate(&batcher_client, &config, &bounds_config, block_timestamp).await;
            match &result {
                Ok(rate) => metrics.record_success(*rate),
                Err(error) => {
                    metrics.record_error(error.into());
                    warn!("Failed {pair:?} query for block timestamp {block_timestamp}: {error:?}");
                }
            }
            result.map(|rate| ValidRead { rate, block_timestamp })
        })
    }

    // Moves a finished query's outcome into `state`: a success becomes the last valid read and
    // clears the last error, a failure becomes the last error. Called on every
    // `fetch_rate`, so that a query which resolved after the last caller that could have observed
    // it is harvested rather than dropped together with the round trip that produced it.
    fn harvest_finished_query(&self, state: &mut PairOracleState) {
        if !state.query.as_ref().is_some_and(|query| query.is_finished()) {
            return;
        }
        let joined = state
            .query
            .take()
            .expect("Query must be present if it reported being finished")
            .now_or_never()
            .expect("Finished query must resolve immediately");
        let result = joined.unwrap_or_else(|join_error| {
            let error = ExchangeRateOracleClientError::JoinError(join_error.to_string());
            self.metrics.record_error((&error).into());
            warn!("Query failed to join its handle: {error:?}");
            Err(error)
        });
        match result {
            Ok(valid_read) => {
                debug!(
                    "Harvested {:?} rate {} for block timestamp {}",
                    Kind::PAIR,
                    valid_read.rate,
                    valid_read.block_timestamp
                );
                state.last_valid_read = Some(valid_read);
                state.last_error = None;
            }
            // `spawn_query` already warned; this only holds it for the retry interval.
            Err(error) => state.last_error = Some(error),
        }
    }
}

#[async_trait]
impl<Kind: ChainlinkRate> ExchangeRateOracleClientTrait for ChainlinkOracleClient<Kind> {
    #[instrument(skip(self))]
    async fn fetch_rate(
        &self,
        block_timestamp: u64,
    ) -> Result<ExchangeRate, ExchangeRateOracleClientError> {
        // Held for the whole function: harvesting the finished query, deciding whether to spawn the
        // next one and serving this caller are one critical section, so no caller can observe a
        // query that was taken but whose outcome is not stored yet.
        let mut state = self.state.lock().unwrap();
        self.harvest_finished_query(&mut state);

        let refresh_interval_seconds = if state.last_error.is_some() {
            self.config.failure_retry_interval_seconds
        } else {
            self.config.sampling_interval_seconds
        };
        let refresh_interval = Duration::from_secs(refresh_interval_seconds);
        let is_refresh_due = state
            .last_attempt_instant
            .is_none_or(|last_attempt| last_attempt.elapsed() >= refresh_interval);
        if state.query.is_none() && is_refresh_due {
            state.query = Some(self.spawn_query(block_timestamp));
            state.last_attempt_instant = Some(Instant::now());
        }

        // A caller whose own read has not resolved is served the last valid read while it is close
        // enough. The distance is measured between block timestamps, in both directions, so a
        // re-proposal for an earlier timestamp reaches the same decision the proposer did.
        let max_fallback_distance_seconds =
            MAX_FALLBACK_SAMPLING_INTERVALS.saturating_mul(self.config.sampling_interval_seconds);
        if let Some(valid_read) = state.last_valid_read.filter(|valid_read| {
            block_timestamp.abs_diff(valid_read.block_timestamp) <= max_fallback_distance_seconds
        }) {
            return Ok(valid_read.rate);
        }
        match state.last_error.clone() {
            Some(error) => Err(error),
            None => Err(ExchangeRateOracleClientError::QueryNotReadyError(block_timestamp)),
        }
    }
}

#[async_trait]
impl ChainlinkRate for StrkToUsd {
    fn metrics() -> ExchangeRateOracleMetrics {
        STRK_TO_USD_ORACLE_METRICS
    }

    async fn query_rate(
        batcher_client: &SharedBatcherClient,
        config: &ChainlinkOracleConfig,
        bounds_config: &AllRateBoundsConfig,
        block_timestamp: u64,
    ) -> RateResult {
        read_feed(batcher_client, config.strk_usd_feed(bounds_config), block_timestamp).await
    }
}

#[async_trait]
impl ChainlinkRate for EthToFri {
    fn metrics() -> ExchangeRateOracleMetrics {
        ETH_TO_STRK_ORACLE_METRICS
    }

    async fn query_rate(
        batcher_client: &SharedBatcherClient,
        config: &ChainlinkOracleConfig,
        bounds_config: &AllRateBoundsConfig,
        block_timestamp: u64,
    ) -> RateResult {
        // The two legs are separate `call_contract` calls, which exposes no block pinning, so they
        // may straddle a block boundary. A one-block skew is orders of magnitude below the
        // staleness bound both legs must independently pass.
        // Sequential, not `try_join`: a failing leg would drop the other mid-flight, and the
        // batcher's local component server panics when a dropped request's response channel
        // closes. The same constraint applies to the two `call_view` calls inside `read_feed`.
        let eth_to_usd_rate =
            read_feed(batcher_client, config.eth_usd_feed(bounds_config), block_timestamp).await?;
        let strk_to_usd_rate =
            read_feed(batcher_client, config.strk_usd_feed(bounds_config), block_timestamp).await?;

        let eth_to_fri_rate = derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate)?;
        check_rate_bounds(eth_to_fri_rate, bounds_config.eth_strk_bounds())?;
        Ok(eth_to_fri_rate)
    }
}
