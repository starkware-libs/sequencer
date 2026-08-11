use std::fmt::{Debug, Formatter, Result as FormatterResult};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

use apollo_batcher_types::batcher_types::CallContractInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_cairo_utils::{deserialize_retdata, RetdataDeserializationError, TryFromIterator};
use apollo_l1_gas_price_config::config::{
    ChainlinkOracleConfig,
    MicroUnitBounds,
    MICRO_UNIT_DECIMALS,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::ExchangeRateOracleClientTrait;
use apollo_metrics::metrics::set_unix_now_seconds;
use async_trait::async_trait;
use futures::future::try_join;
use futures::FutureExt;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoStaticStr, VariantNames};
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, info, instrument, warn};

use crate::metrics::{
    register_chainlink_guard_metrics,
    ExchangeRateOracleMetrics,
    CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ETH_TO_STRK_ORACLE_METRICS,
    LABEL_NAME_CHAINLINK_FEED,
    STRK_TO_USD_ORACLE_METRICS,
};

#[cfg(test)]
#[path = "chainlink_oracle_test.rs"]
mod chainlink_oracle_test;

const LATEST_ROUND_DATA_ENTRY_POINT: &str = "latest_round_data";
const DECIMALS_ENTRY_POINT: &str = "decimals";

/// Feeds, readings and rates are all named by their currency pair.
const ETH_USD_PAIR_NAME: &str = "ETH/USD";
const STRK_USD_PAIR_NAME: &str = "STRK/USD";
/// Derived from the two USD pairs: no Chainlink feed on Starknet quotes ETH in STRK.
/// `RATE_DECIMALS` is 18 and one STRK is 10^18 FRI, so FRI per ETH at `RATE_DECIMALS` and STRK per
/// ETH are numerically the same value.
const ETH_STRK_PAIR_NAME: &str = "ETH/STRK";

/// Which reading a guard rejected. It is what the guard counters are labeled by, so a rejected
/// ETH/USD leg and a rejected STRK/USD leg land on separate series.
#[derive(Clone, Copy, Debug, EnumIter, IntoStaticStr, PartialEq, Eq, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ChainlinkFeed {
    EthUsd,
    StrkUsd,
    /// Not a feed: the rate derived from the two USD feeds, whose bounds are checked on their own.
    EthStrk,
}

impl ChainlinkFeed {
    fn pair_name(self) -> &'static str {
        match self {
            ChainlinkFeed::EthUsd => ETH_USD_PAIR_NAME,
            ChainlinkFeed::StrkUsd => STRK_USD_PAIR_NAME,
            ChainlinkFeed::EthStrk => ETH_STRK_PAIR_NAME,
        }
    }

    fn labels(self) -> [(&'static str, &'static str); 1] {
        [(LABEL_NAME_CHAINLINK_FEED, self.into())]
    }
}

/// Fixed-point scale of every rate this client returns, matching `EXCHANGE_RATE_DECIMALS`.
const RATE_DECIMALS: u32 = 18;
const RATE_SCALE: u128 = 10u128.pow(RATE_DECIMALS);
const MICRO_UNIT_TO_RATE_SCALE: u128 = 10u128.pow(RATE_DECIMALS - MICRO_UNIT_DECIMALS);

/// The Chainlink feeds report 8 decimals today. A range is accepted rather than the exact value so
/// that a feed upgrade does not halt pricing, bounded so the rescale to `RATE_DECIMALS` can
/// neither underflow nor produce an absurd scale factor.
const MIN_FEED_DECIMALS: u32 = 6;
const MAX_FEED_DECIMALS: u32 = RATE_DECIMALS;

/// Cap on the batcher error text this client relays. A reverting view call's panic data reaches
/// the logs, the failure cache, and (when the provider runs remotely) the RPC boundary, so the cap
/// is byte-based to bound what all three consume.
const MAX_CONTRACT_CALL_ERROR_BYTES: usize = 256;
const TRUNCATION_MARKER: &str = "...[truncated]";

/// A rate at `RATE_DECIMALS`, or the guard trip that rejected it.
type RateResult = Result<u128, ExchangeRateOracleClientError>;
type RateQuery = AbortOnDropHandle<RateResult>;

/// How many `lag_interval_seconds` past its own bucket the last valid rate may still be served,
/// when the bucket being queried has no rate of its own. Three intervals covers three consecutive
/// failed reads, and stays far inside `max_staleness_seconds`, which bounds how far the price
/// itself may have moved.
const MAX_FALLBACK_LAG_INTERVALS: u64 = 3;

/// Which rate a `ChainlinkOracleClient` instance produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainlinkRateKind {
    /// USD per STRK, read directly from the STRK/USD feed.
    StrkToUsd,
    /// FRI per ETH, derived from the ETH/USD and STRK/USD feeds. Starknet has no ETH/STRK feed.
    EthToFri,
}

/// A rate that passed every guard, and the bucket timestamp it was read for. The timestamp is what
/// makes the rate's age checkable once later buckets have no rate of their own.
#[derive(Clone, Copy, Debug)]
struct ValidRead {
    rate: u128,
    query_timestamp: u64,
}

/// The newest bucket `fetch_rate` has been called for.
struct CurrentBucket {
    query_timestamp: u64,
    /// The bucket's own result, once its query resolves. Holding a failure keeps a broken feed to
    /// one query per bucket rather than one per proposal.
    result: Option<RateResult>,
    /// Taken once the query resolves, so a resolved handle is not polled again. A single slot per
    /// client bounds queries in flight: replacing it drops the previous `AbortOnDropHandle`, which
    /// aborts the superseded query.
    query: Option<RateQuery>,
}

#[derive(Default)]
struct OracleState {
    last_valid_read: Option<ValidRead>,
    current_bucket: Option<CurrentBucket>,
}

/// Reads Chainlink's on-chain Starknet price feeds through the sequencer's own batcher.
///
/// Consensus calls `fetch_rate` on every proposal build and validate, so the call must not block on
/// the batcher: a bucket without a result of its own spawns a background query and immediately
/// falls back to the last valid read, while that read is within `MAX_FALLBACK_LAG_INTERVALS` of the
/// bucket being queried. A bucket is one `lag_interval_seconds` window, identified by the
/// `query_timestamp` its query is issued for.
///
/// Reads are not deterministic across nodes: `call_contract` executes against the batcher's latest
/// committed block rather than state pinned to the queried timestamp, so two nodes can read
/// different rounds for the same logical bucket. Chainlink's deviation threshold is far inside the
/// `l1_gas_price_margin_percent` validators compare within, so this is not expected to reject
/// proposals.
#[derive(Clone)]
pub struct ChainlinkOracleClient {
    rate_kind: ChainlinkRateKind,
    config: ChainlinkOracleConfig,
    lag_interval_seconds: NonZeroU64,
    batcher_client: SharedBatcherClient,
    state: Arc<Mutex<OracleState>>,
    metrics: ExchangeRateOracleMetrics,
}

// Manual impl: the trait requires `Debug` but `SharedBatcherClient` does not provide it.
impl Debug for ChainlinkOracleClient {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatterResult {
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
        Self {
            rate_kind,
            lag_interval_seconds: NonZeroU64::new(config.lag_interval_seconds)
                .expect("lag_interval_seconds must be non-zero"),
            config,
            batcher_client,
            state: Arc::new(Mutex::new(OracleState::default())),
            metrics,
        }
    }

    /// `query_timestamp` is the start of the bucket being queried, not the caller's timestamp, so
    /// that the freshness window a cached result was accepted under is the same for every caller
    /// in the bucket.
    fn spawn_query(&self, query_timestamp: u64) -> RateQuery {
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

    /// Moves the bucket's query result into the bucket, once that query has resolved.
    fn store_resolved_result(&self, bucket: &mut CurrentBucket) {
        if !bucket.query.as_ref().is_some_and(|query| query.is_finished()) {
            return;
        }
        let query_timestamp = bucket.query_timestamp;
        let joined = bucket
            .query
            .take()
            .expect("Query must be present if it reported being finished")
            .now_or_never()
            .expect("Finished query must resolve immediately");
        let result = joined.unwrap_or_else(|error| {
            warn!("Query failed to join handle for timestamp {query_timestamp}: {error:?}");
            self.metrics.error_count.increment(1);
            Err(ExchangeRateOracleClientError::JoinError(error.to_string()))
        });
        debug!("Storing result for timestamp {query_timestamp}: {result:?}");
        bucket.result = Some(result);
    }
}

#[async_trait]
impl ExchangeRateOracleClientTrait for ChainlinkOracleClient {
    #[instrument(skip(self))]
    async fn fetch_rate(&self, timestamp: u64) -> Result<u128, ExchangeRateOracleClientError> {
        let lag_interval_seconds = self.lag_interval_seconds.get();
        // Quantized to a bucket one interval back, matching `ExchangeRateOracleClient`: the
        // interval `[T, T+k)` serves queries in `[T+k, T+2k)`.
        let query_timestamp = timestamp.saturating_sub(lag_interval_seconds)
            / self.lag_interval_seconds
            * lag_interval_seconds;

        // Held for the whole function: the held-result check, the resolved query's removal and the
        // storing of its result are one critical section, so no other caller can observe a bucket
        // whose query was removed but whose result is not yet stored.
        let mut state = self.state.lock().unwrap();
        let OracleState { last_valid_read, current_bucket } = &mut *state;

        // A later bucket supersedes the one held; dropping its handle aborts a query still in
        // flight. An earlier bucket, which a re-proposal can ask for, is served from what is
        // already held rather than spawning a query for a bucket already left behind.
        if current_bucket.as_ref().is_none_or(|bucket| bucket.query_timestamp < query_timestamp) {
            *current_bucket = Some(CurrentBucket {
                query_timestamp,
                result: None,
                query: Some(self.spawn_query(query_timestamp)),
            });
        }
        let bucket = current_bucket.as_mut().expect("Bucket is set above when empty");

        let bucket_result = if bucket.query_timestamp == query_timestamp {
            self.store_resolved_result(bucket);
            bucket.result.clone()
        } else {
            None
        };
        if let Some(Ok(rate)) = bucket_result {
            *last_valid_read = Some(ValidRead { rate, query_timestamp });
            return Ok(rate);
        }

        // A bucket with no rate of its own serves the last valid read while it is recent enough.
        // Age is measured between bucket timestamps rather than against wall clock, so the
        // decision is the same on every node for a given block timestamp.
        let max_fallback_age_seconds =
            MAX_FALLBACK_LAG_INTERVALS.saturating_mul(lag_interval_seconds);
        if let Some(valid_read) = last_valid_read.filter(|valid_read| {
            query_timestamp.saturating_sub(valid_read.query_timestamp) <= max_fallback_age_seconds
        }) {
            debug!(
                "No rate for timestamp {timestamp}, using rate {} from timestamp={}",
                valid_read.rate, valid_read.query_timestamp
            );
            return Ok(valid_read.rate);
        }
        match bucket_result {
            Some(Err(error)) => Err(error),
            _ => Err(ExchangeRateOracleClientError::QueryNotReadyError(timestamp)),
        }
    }
}

// TODO(Asaf): bound the rate's change against the previous block's implied rate. The absolute
// bounds below are wide enough to pass a manipulated but plausible answer (the STRK/USD band spans
// five decades), which only a bound relative to the last accepted rate catches. It must be anchored
// to the block header rather than to node-local history, so that every validator accepts and
// rejects the same values.
async fn query_rate(
    batcher_client: &SharedBatcherClient,
    config: &ChainlinkOracleConfig,
    rate_kind: ChainlinkRateKind,
    query_timestamp: u64,
) -> RateResult {
    match rate_kind {
        ChainlinkRateKind::StrkToUsd => {
            read_feed(
                batcher_client,
                config,
                config.strk_usd_feed_address,
                &config.strk_usd_price_bounds,
                ChainlinkFeed::StrkUsd,
                query_timestamp,
            )
            .await
        }
        ChainlinkRateKind::EthToFri => {
            // The two legs are separate `call_contract` calls, which exposes no block pinning, so
            // they may straddle a block boundary. A one-block skew is orders of magnitude below
            // the staleness bound both legs must independently pass.
            let (eth_to_usd_rate, strk_to_usd_rate) = try_join(
                read_feed(
                    batcher_client,
                    config,
                    config.eth_usd_feed_address,
                    &config.eth_usd_price_bounds,
                    ChainlinkFeed::EthUsd,
                    query_timestamp,
                ),
                read_feed(
                    batcher_client,
                    config,
                    config.strk_usd_feed_address,
                    &config.strk_usd_price_bounds,
                    ChainlinkFeed::StrkUsd,
                    query_timestamp,
                ),
            )
            .await?;

            let eth_to_fri_rate = derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate)?;
            check_rate_bounds(
                eth_to_fri_rate,
                &config.eth_to_fri_rate_bounds,
                ChainlinkFeed::EthStrk,
            )?;
            Ok(eth_to_fri_rate)
        }
    }
}

/// The feed's answer, rescaled to `RATE_DECIMALS` and checked against `bounds`.
async fn read_feed(
    batcher_client: &SharedBatcherClient,
    config: &ChainlinkOracleConfig,
    feed_address: ContractAddress,
    bounds: &MicroUnitBounds,
    feed: ChainlinkFeed,
    query_timestamp: u64,
) -> RateResult {
    let pair_name = feed.pair_name();
    // `decimals()` is re-read every round rather than cached once, because a cached value that the
    // feed has since changed rescales the answer by a power of ten, and the absolute bounds are too
    // wide to catch that: the STRK/USD band spans five decades on purpose, so reading an 8-decimal
    // answer as 6 decimals turns $0.03 into $3.00 and passes.
    let (decimals_retdata, round_retdata) = try_join(
        call_view(batcher_client, feed_address, DECIMALS_ENTRY_POINT, feed),
        call_view(batcher_client, feed_address, LATEST_ROUND_DATA_ENTRY_POINT, feed),
    )
    .await?;
    let feed_decimals = decode_feed_decimals(decimals_retdata, feed)?;

    let round: ChainlinkRoundData = decode_retdata(round_retdata, feed)?;
    if round.answer == 0 {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &feed.labels());
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} returned a zero answer"
        )));
    }
    if query_timestamp.saturating_sub(round.updated_at) > config.max_staleness_seconds {
        CHAINLINK_ORACLE_STALE_FEED_COUNT.increment(1, &feed.labels());
        return Err(ExchangeRateOracleClientError::StaleFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            query_timestamp,
            max_staleness_seconds: config.max_staleness_seconds,
        });
    }
    // Catches a round dated ahead of the query: the staleness check above saturates such a
    // subtraction to zero, which alone treats it as fresh regardless of age.
    if round.updated_at.saturating_sub(query_timestamp) > config.max_future_updated_at_seconds {
        CHAINLINK_ORACLE_FUTURE_FEED_COUNT.increment(1, &feed.labels());
        return Err(ExchangeRateOracleClientError::FutureFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            query_timestamp,
            max_future_updated_at_seconds: config.max_future_updated_at_seconds,
        });
    }

    let rate = rescale_to_rate_decimals(round.answer, feed_decimals)?;
    check_rate_bounds(rate, bounds, feed)?;
    Ok(rate)
}

async fn call_view(
    batcher_client: &SharedBatcherClient,
    contract_address: ContractAddress,
    entry_point: &str,
    feed: ChainlinkFeed,
) -> Result<Vec<Felt>, ExchangeRateOracleClientError> {
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
            CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1, &feed.labels());
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
    feed: ChainlinkFeed,
) -> Result<u32, ExchangeRateOracleClientError> {
    let pair_name = feed.pair_name();
    let raw_decimals: Felt = decode_retdata(decimals_retdata, feed)?;
    let feed_decimals = u32::try_from(raw_decimals).map_err(|_| {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &feed.labels());
        ExchangeRateOracleClientError::ParseError(format!(
            "{pair_name} decimals {raw_decimals} does not fit in u32"
        ))
    })?;
    if !(MIN_FEED_DECIMALS..=MAX_FEED_DECIMALS).contains(&feed_decimals) {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &feed.labels());
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} reports {feed_decimals} decimals, outside the accepted range \
             [{MIN_FEED_DECIMALS}, {MAX_FEED_DECIMALS}]"
        )));
    }
    Ok(feed_decimals)
}

fn decode_retdata<T>(
    retdata: Vec<Felt>,
    feed: ChainlinkFeed,
) -> Result<T, ExchangeRateOracleClientError>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    deserialize_retdata(retdata).map_err(|error| {
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1, &feed.labels());
        ExchangeRateOracleClientError::ParseError(error.to_string())
    })
}

fn rescale_to_rate_decimals(answer: u128, feed_decimals: u32) -> RateResult {
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
fn derive_eth_to_fri_rate(eth_to_usd_rate: u128, strk_to_usd_rate: u128) -> RateResult {
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
    bounds: &MicroUnitBounds,
    feed: ChainlinkFeed,
) -> Result<(), ExchangeRateOracleClientError> {
    let min_rate = u128::from(bounds.minimum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    let max_rate = u128::from(bounds.maximum_micro_units).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    if rate < min_rate || rate > max_rate {
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.increment(1, &feed.labels());
        return Err(ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair_name: feed.pair_name().to_string(),
            rate,
            min_rate,
            max_rate,
        });
    }
    Ok(())
}

/// The fields of Chainlink's `Round` that this client consumes.
struct ChainlinkRoundData {
    /// The price the feed reports, at the feed's own `decimals()`.
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
