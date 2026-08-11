use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::{Arc, Mutex};

use apollo_batcher_types::batcher_types::CallContractInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_cairo_utils::{RetdataDeserializationError, TryFromIterator};
use apollo_l1_gas_price_config::config::ChainlinkOracleConfig;
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::ExchangeRateOracleClientTrait;
use apollo_metrics::metrics::set_unix_now_seconds;
use async_trait::async_trait;
use futures::future::try_join;
use futures::FutureExt;
use lru::LruCache;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, info, instrument, warn};

use crate::metrics::{
    register_chainlink_guard_metrics,
    ExchangeRateOracleMetrics,
    CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ETH_TO_STRK_ORACLE_METRICS,
    STRK_TO_USD_ORACLE_METRICS,
};

#[cfg(test)]
#[path = "chainlink_oracle_test.rs"]
mod chainlink_oracle_test;

const LATEST_ROUND_DATA_ENTRY_POINT: &str = "latest_round_data";
const DECIMALS_ENTRY_POINT: &str = "decimals";

/// Feeds, readings and rates are all named by their currency pair: the ETH/USD feed reports the
/// ETH/USD rate, which the config bounds as `min/max_eth_usd_price_micro_usd`.
const ETH_USD_PAIR_NAME: &str = "ETH/USD";
const STRK_USD_PAIR_NAME: &str = "STRK/USD";
/// The derived pair. `RATE_DECIMALS` is 18 and one STRK is 10^18 FRI, so FRI per ETH at
/// `RATE_DECIMALS` and STRK per ETH are numerically the same value.
const ETH_STRK_PAIR_NAME: &str = "ETH/STRK";

/// Fixed-point scale of every rate this client returns, matching `EXCHANGE_RATE_DECIMALS`.
const RATE_DECIMALS: u32 = 18;
const RATE_SCALE: u128 = 10u128.pow(RATE_DECIMALS);
/// Scale of the micro-unit sanity bounds in `ChainlinkOracleConfig`.
const MICRO_UNIT_DECIMALS: u32 = 6;
const MICRO_UNIT_TO_RATE_SCALE: u128 = 10u128.pow(RATE_DECIMALS - MICRO_UNIT_DECIMALS);

/// The Starknet feeds report 8 decimals today. A range is accepted rather than the exact value so
/// that a feed upgrade does not halt pricing, bounded so the rescale to `RATE_DECIMALS` can
/// neither underflow nor produce an absurd scale factor.
const MIN_FEED_DECIMALS: u32 = 6;
const MAX_FEED_DECIMALS: u32 = RATE_DECIMALS;

/// Cap on the batcher error text this client relays. For a reverting view call that text is the
/// feed contract's own panic data, which reaches the logs, the cache of failed results and, when
/// the provider runs remotely, the component RPC boundary. The cap counts bytes because bytes are
/// what those three consume.
const MAX_CONTRACT_CALL_ERROR_BYTES: usize = 256;
const TRUNCATION_MARKER: &str = "...[truncated]";

type RateQuery = AbortOnDropHandle<Result<u128, ExchangeRateOracleClientError>>;

/// Which rate a `ChainlinkOracleClient` instance produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainlinkRateKind {
    /// USD per STRK, read directly from the STRK/USD feed.
    StrkToUsd,
    /// FRI per ETH, derived from the ETH/USD and STRK/USD feeds. Starknet has no ETH/STRK feed.
    EthToFri,
}

/// Reads Chainlink's on-chain Starknet price feeds through the sequencer's own batcher.
///
/// Consensus calls `fetch_rate` on every proposal build and validate, so the call must not block on
/// the batcher: a miss spawns a background query and immediately falls back to the previous
/// bucket's cached rate, mirroring `ExchangeRateOracleClient`. A bucket is one
/// `lag_interval_seconds` window, identified by the `quantized_timestamp` its query is issued for.
///
/// Unlike the HTTP oracle, whose per-timestamp URL returned one value to every node, these reads
/// are not deterministic across nodes: `call_contract` executes against the batcher's latest
/// committed block rather than state pinned to the queried timestamp, and whether a node holds
/// bucket N or N-1 depends on its own history. Two nodes can therefore read different rounds for
/// the same logical bucket. Chainlink's deviation threshold is far inside the
/// `l1_gas_price_margin_percent` validators compare within, so this is not expected to reject
/// proposals.
// TODO(Asaf): pin both feed reads to a block, once the batcher can execute a view call against a
// caller-chosen block rather than its latest committed one.
#[derive(Clone)]
pub struct ChainlinkOracleClient {
    rate_kind: ChainlinkRateKind,
    config: ChainlinkOracleConfig,
    lag_interval_seconds: NonZeroU64,
    batcher_client: SharedBatcherClient,
    /// Failures are cached alongside successes, so a broken feed is queried once per bucket rather
    /// than on every proposal. A cached failure never masks the previous bucket's rate.
    cached_results: Arc<Mutex<LruCache<u64, Result<u128, ExchangeRateOracleClientError>>>>,
    queries: Arc<Mutex<LruCache<u64, RateQuery>>>,
    metrics: ExchangeRateOracleMetrics,
}

// Manual impl: the trait requires `Debug` but `SharedBatcherClient` does not provide it.
impl std::fmt::Debug for ChainlinkOracleClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChainlinkOracleClient")
            .field("rate_kind", &self.rate_kind)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ChainlinkOracleClient {
    pub fn new(
        rate_kind: ChainlinkRateKind,
        config: ChainlinkOracleConfig,
        batcher_client: SharedBatcherClient,
    ) -> Self {
        info!(
            "Creating ChainlinkOracleClient for {rate_kind:?} with: eth_usd_feed_address={} \
             strk_usd_feed_address={} max_staleness_seconds={} max_future_updated_at_seconds={} \
             lag_interval_seconds={}",
            config.eth_usd_feed_address,
            config.strk_usd_feed_address,
            config.max_staleness_seconds,
            config.max_future_updated_at_seconds,
            config.lag_interval_seconds,
        );
        let metrics = match rate_kind {
            ChainlinkRateKind::StrkToUsd => STRK_TO_USD_ORACLE_METRICS,
            ChainlinkRateKind::EthToFri => ETH_TO_STRK_ORACLE_METRICS,
        };
        metrics.register();
        register_chainlink_guard_metrics();
        let cache_size =
            NonZeroUsize::new(config.max_cache_size).expect("max_cache_size must be non-zero");
        Self {
            rate_kind,
            lag_interval_seconds: NonZeroU64::new(config.lag_interval_seconds)
                .expect("lag_interval_seconds must be non-zero"),
            config,
            batcher_client,
            cached_results: Arc::new(Mutex::new(LruCache::new(cache_size))),
            queries: Arc::new(Mutex::new(LruCache::new(cache_size))),
            metrics,
        }
    }

    fn spawn_query(&self, quantized_timestamp: u64) -> RateQuery {
        let query_timestamp = quantized_timestamp.saturating_mul(self.lag_interval_seconds.get());
        let batcher_client = self.batcher_client.clone();
        let config = self.config.clone();
        let rate_kind = self.rate_kind;
        let metrics = self.metrics;
        AbortOnDropHandle::new(tokio::spawn(async move {
            let result = query_rate(&batcher_client, &config, rate_kind, query_timestamp).await;
            match &result {
                Ok(rate) => {
                    metrics.success_count.increment(1);
                    set_unix_now_seconds(metrics.last_success_timestamp);
                    metrics.rate.set_lossy(*rate);
                    debug!("Resolved {rate_kind:?} query for {query_timestamp} to {rate}");
                }
                Err(error) => {
                    metrics.error_count.increment(1);
                    warn!("Failed {rate_kind:?} query for {query_timestamp}: {error:?}");
                }
            }
            result
        }))
    }
}

#[async_trait]
impl ExchangeRateOracleClientTrait for ChainlinkOracleClient {
    #[instrument(skip(self))]
    async fn fetch_rate(&self, timestamp: u64) -> Result<u128, ExchangeRateOracleClientError> {
        const NUMBER_OF_TIMESTAMPS_BACK: u64 = 1;
        let quantized_timestamp =
            timestamp.saturating_sub(self.lag_interval_seconds.get()) / self.lag_interval_seconds;
        let previous_timestamp = quantized_timestamp.saturating_sub(NUMBER_OF_TIMESTAMPS_BACK);

        let mut cache = self.cached_results.lock().unwrap();
        if let Some(cached_result) = cache.get(&quantized_timestamp).cloned() {
            debug!("Cached result for timestamp {timestamp} is {cached_result:?}");
            let cached_error = match cached_result {
                Ok(rate) => return Ok(rate),
                Err(error) => error,
            };
            // Caching the failure bounds the call volume against the batcher, but it must not deny
            // the proposal path a rate this client already holds: consensus is better served by a
            // slightly stale rate than by the freshest possible error.
            if let Some(Ok(rate)) = cache.get(&previous_timestamp) {
                debug!(
                    "Cached failure for timestamp {timestamp}, using previous rate {rate} from \
                     quantized timestamp={previous_timestamp}"
                );
                return Ok(*rate);
            }
            return Err(cached_error);
        }

        // Reuse the query already in flight for this bucket, or start one.
        let mut queries = self.queries.lock().unwrap();
        let handle = queries
            .get_or_insert_mut(quantized_timestamp, || self.spawn_query(quantized_timestamp));
        if !handle.is_finished() {
            if let Some(Ok(rate)) = cache.get(&previous_timestamp) {
                debug!(
                    "Query not yet resolved: timestamp={timestamp}, using previous rate {rate} \
                     from quantized timestamp={previous_timestamp}"
                );
                return Ok(*rate);
            }
            return Err(ExchangeRateOracleClientError::QueryNotReadyError(timestamp));
        }

        let joined = handle.now_or_never().expect("Handle must be finished if we got here");
        // Must remove the resolved query, to avoid re-polling a resolved handle.
        queries.pop(&quantized_timestamp);
        let result = joined.unwrap_or_else(|error| {
            warn!("Query failed to join handle for timestamp {timestamp}: {error:?}");
            self.metrics.error_count.increment(1);
            Err(ExchangeRateOracleClientError::JoinError(error.to_string()))
        });

        debug!("Caching result for timestamp {timestamp}: {result:?}");
        cache.put(quantized_timestamp, result.clone());
        result
    }
}

// TODO(Asaf): bound the rate's change against the previous block's implied rate, anchored to the
// block header so that every validator accepts and rejects the same values.
async fn query_rate(
    batcher_client: &SharedBatcherClient,
    config: &ChainlinkOracleConfig,
    rate_kind: ChainlinkRateKind,
    query_timestamp: u64,
) -> Result<u128, ExchangeRateOracleClientError> {
    match rate_kind {
        ChainlinkRateKind::StrkToUsd => {
            let strk_usd_reading = read_feed(
                batcher_client,
                config,
                config.strk_usd_feed_address,
                STRK_USD_PAIR_NAME,
                query_timestamp,
            )
            .await?;
            let strk_to_usd_rate =
                rescale_to_rate_decimals(strk_usd_reading.answer, strk_usd_reading.feed_decimals)?;
            check_rate_bounds(
                strk_to_usd_rate,
                config.min_strk_usd_price_micro_usd,
                config.max_strk_usd_price_micro_usd,
                STRK_USD_PAIR_NAME,
            )?;
            Ok(strk_to_usd_rate)
        }
        ChainlinkRateKind::EthToFri => {
            // The two legs are separate `call_contract` calls, which exposes no block pinning, so
            // they may straddle a block boundary. A one-block skew is orders of magnitude below
            // the staleness bound both legs must independently pass.
            let (eth_usd_reading, strk_usd_reading) = try_join(
                read_feed(
                    batcher_client,
                    config,
                    config.eth_usd_feed_address,
                    ETH_USD_PAIR_NAME,
                    query_timestamp,
                ),
                read_feed(
                    batcher_client,
                    config,
                    config.strk_usd_feed_address,
                    STRK_USD_PAIR_NAME,
                    query_timestamp,
                ),
            )
            .await?;

            let eth_to_usd_rate =
                rescale_to_rate_decimals(eth_usd_reading.answer, eth_usd_reading.feed_decimals)?;
            check_rate_bounds(
                eth_to_usd_rate,
                config.min_eth_usd_price_micro_usd,
                config.max_eth_usd_price_micro_usd,
                ETH_USD_PAIR_NAME,
            )?;
            let strk_to_usd_rate =
                rescale_to_rate_decimals(strk_usd_reading.answer, strk_usd_reading.feed_decimals)?;
            check_rate_bounds(
                strk_to_usd_rate,
                config.min_strk_usd_price_micro_usd,
                config.max_strk_usd_price_micro_usd,
                STRK_USD_PAIR_NAME,
            )?;

            let eth_to_fri_rate = derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate)?;
            check_rate_bounds(
                eth_to_fri_rate,
                config.min_eth_to_fri_rate_micro_strk,
                config.max_eth_to_fri_rate_micro_strk,
                ETH_STRK_PAIR_NAME,
            )?;
            Ok(eth_to_fri_rate)
        }
    }
}

async fn read_feed(
    batcher_client: &SharedBatcherClient,
    config: &ChainlinkOracleConfig,
    feed_address: ContractAddress,
    pair_name: &str,
    query_timestamp: u64,
) -> Result<FeedReading, ExchangeRateOracleClientError> {
    // `decimals()` is re-read with every round rather than kept for the client's lifetime. Caching
    // it would save one view call per bucket, but it would also pin a single wrong read until the
    // node restarts, and the absolute bounds cannot be relied on to notice: the STRK/USD band
    // spans five decades on purpose, so 8 decimals misread as 6 turns $0.03 into $3.00 and passes.
    let (decimals_retdata, round_retdata) = try_join(
        call_view(batcher_client, feed_address, DECIMALS_ENTRY_POINT),
        call_view(batcher_client, feed_address, LATEST_ROUND_DATA_ENTRY_POINT),
    )
    .await?;
    let feed_decimals = decode_feed_decimals(decimals_retdata, pair_name)?;

    let round: ChainlinkRoundData = decode_retdata(round_retdata)?;
    if round.answer == 0 {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} returned a zero answer"
        )));
    }
    if query_timestamp.saturating_sub(round.updated_at) > config.max_staleness_seconds {
        CHAINLINK_ORACLE_STALE_FEED_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::StaleFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            query_timestamp,
            max_staleness_seconds: config.max_staleness_seconds,
        });
    }
    // Without this the staleness check is vacuous in one direction: a poisoned round dated far in
    // the future saturates the subtraction above to zero and stays "fresh" forever.
    if round.updated_at.saturating_sub(query_timestamp) > config.max_future_updated_at_seconds {
        CHAINLINK_ORACLE_STALE_FEED_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::FutureFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            query_timestamp,
            max_future_updated_at_seconds: config.max_future_updated_at_seconds,
        });
    }

    Ok(FeedReading { answer: round.answer, feed_decimals })
}

async fn call_view(
    batcher_client: &SharedBatcherClient,
    contract_address: ContractAddress,
    entry_point: &str,
) -> Result<Vec<Felt>, ExchangeRateOracleClientError> {
    // TODO(Asaf): bound the execution of these view calls in the batcher. Without a step limit a
    // hostile feed contract can stall the batcher's request loop; this client caps only the number
    // of calls it makes.
    let call_result = batcher_client
        .call_contract(CallContractInput {
            contract_address,
            entry_point: entry_point.to_string(),
            calldata: vec![],
        })
        .await;
    match call_result {
        Ok(output) => Ok(output.retdata),
        Err(error) => {
            CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1);
            Err(ExchangeRateOracleClientError::ContractCallError(format!(
                "{entry_point} at {contract_address}: {}",
                truncate_contract_call_error(error.to_string())
            )))
        }
    }
}

fn truncate_contract_call_error(error_text: String) -> String {
    if error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES {
        return error_text;
    }
    // Cut on a character boundary so the relayed text stays valid UTF-8. The nearest boundary at
    // or below the cap is at most three bytes down.
    let head_end = (0..=MAX_CONTRACT_CALL_ERROR_BYTES)
        .rev()
        .find(|byte_index| error_text.is_char_boundary(*byte_index))
        .expect("Byte index 0 is always a character boundary");
    format!("{}{TRUNCATION_MARKER}", &error_text[..head_end])
}

fn decode_feed_decimals(
    decimals_retdata: Vec<Felt>,
    pair_name: &str,
) -> Result<u32, ExchangeRateOracleClientError> {
    let raw_decimals: Felt = decode_retdata(decimals_retdata)?;
    let feed_decimals = u32::try_from(raw_decimals).map_err(|_| {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1);
        ExchangeRateOracleClientError::ParseError(format!(
            "{pair_name} decimals {raw_decimals} does not fit in u32"
        ))
    })?;
    if !(MIN_FEED_DECIMALS..=MAX_FEED_DECIMALS).contains(&feed_decimals) {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} reports {feed_decimals} decimals, outside the accepted range \
             [{MIN_FEED_DECIMALS}, {MAX_FEED_DECIMALS}]"
        )));
    }
    Ok(feed_decimals)
}

fn decode_retdata<T>(retdata: Vec<Felt>) -> Result<T, ExchangeRateOracleClientError>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    let mut felt_iterator = retdata.into_iter();
    let decoded = T::try_from_iter(&mut felt_iterator).map_err(|error| {
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1);
        ExchangeRateOracleClientError::ParseError(error.to_string())
    })?;
    if felt_iterator.next().is_some() {
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::ParseError(
            "unconsumed felts in price feed retdata".to_string(),
        ));
    }
    Ok(decoded)
}

fn rescale_to_rate_decimals(
    answer: u128,
    feed_decimals: u32,
) -> Result<u128, ExchangeRateOracleClientError> {
    RATE_DECIMALS
        .checked_sub(feed_decimals)
        .and_then(|exponent| 10u128.checked_pow(exponent))
        .and_then(|scale| answer.checked_mul(scale))
        .ok_or_else(|| {
            ExchangeRateOracleClientError::ArithmeticError(format!(
                "rescaling answer {answer} from {feed_decimals} to {RATE_DECIMALS} decimals \
                 overflowed"
            ))
        })
}

/// STRK per ETH, at `RATE_DECIMALS`, from two USD prices that already carry `RATE_DECIMALS`.
fn derive_eth_to_fri_rate(
    eth_to_usd_rate: u128,
    strk_to_usd_rate: u128,
) -> Result<u128, ExchangeRateOracleClientError> {
    // The division cancels the two operands' scales, so the result must be scaled back up by
    // `RATE_SCALE`. Scaling the numerator up front overflows u128, so the integer quotient and the
    // remainder are scaled separately and recombined, which is exact: for
    // `eth = quotient * strk + remainder`, `floor(eth * S / strk) = quotient * S +
    // floor(remainder * S / strk)`.
    let scaled_quotient = eth_to_usd_rate
        .checked_div(strk_to_usd_rate)
        .and_then(|quotient| quotient.checked_mul(RATE_SCALE));
    let scaled_remainder = eth_to_usd_rate
        .checked_rem(strk_to_usd_rate)
        .and_then(|remainder| remainder.checked_mul(RATE_SCALE))
        .and_then(|scaled_remainder| scaled_remainder.checked_div(strk_to_usd_rate));
    scaled_quotient
        .zip(scaled_remainder)
        .and_then(|(quotient, remainder)| quotient.checked_add(remainder))
        .ok_or_else(|| {
            ExchangeRateOracleClientError::ArithmeticError(format!(
                "deriving ETH/STRK from eth_to_usd_rate={eth_to_usd_rate} and \
                 strk_to_usd_rate={strk_to_usd_rate} overflowed"
            ))
        })
}

/// Absolute bounds are the only defense against a feed wired to the wrong asset or a
/// plausible-but-poisoned answer: consensus checks that validators agree with each other, never
/// that the agreed value is sane, and every node reads the same chain state.
fn check_rate_bounds(
    rate: u128,
    minimum_micro_units: u64,
    maximum_micro_units: u64,
    pair_name: &str,
) -> Result<(), ExchangeRateOracleClientError> {
    let min_rate = u128::from(minimum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    let max_rate = u128::from(maximum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    if rate < min_rate || rate > max_rate {
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.increment(1);
        return Err(ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair_name: pair_name.to_string(),
            rate,
            min_rate,
            max_rate,
        });
    }
    Ok(())
}

struct FeedReading {
    answer: u128,
    feed_decimals: u32,
}

/// The fields of Chainlink's `Round` that this client consumes.
struct ChainlinkRoundData {
    answer: u128,
    /// Unix seconds at which the aggregator last wrote this round.
    updated_at: u64,
}

impl TryFromIterator<Felt> for ChainlinkRoundData {
    type Error = RetdataDeserializationError;

    // `latest_round_data` returns `Round { round_id: felt252, answer: u128, block_num: u64,
    // started_at: u64, updated_at: u64 }`, serialized flat as exactly five felts in that order.
    fn try_from_iter<T: Iterator<Item = Felt>>(iter: &mut T) -> Result<Self, Self::Error> {
        // `round_id` is phase-encoded as `(phase_id << 128) | aggregator_round_id`, so it exceeds
        // every primitive integer type and is consumed without being decoded.
        let _round_id = Felt::try_from_iter(iter)?;
        // `answer` is unsigned on the Starknet feeds, so there is no sign extension to undo.
        let raw_answer = Felt::try_from_iter(iter)?;
        let answer = u128::try_from(raw_answer)
            .map_err(|_| RetdataDeserializationError::U128ConversionError { felt: raw_answer })?;
        let _block_number = Felt::try_from_iter(iter)?;
        // `started_at` is consumed without being decoded: an aggregator that can lie about
        // `updated_at` can lie about `started_at` too, so `started_at <= updated_at` adds no
        // guarantee beyond the freshness window enforced on `updated_at`.
        let _started_at = Felt::try_from_iter(iter)?;
        let raw_updated_at = Felt::try_from_iter(iter)?;
        let updated_at = u64::try_from(raw_updated_at).map_err(|_| {
            RetdataDeserializationError::U64ConversionError { felt: raw_updated_at }
        })?;
        Ok(Self { answer, updated_at })
    }
}
