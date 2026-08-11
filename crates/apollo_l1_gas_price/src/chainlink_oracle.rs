use std::fmt::{Debug, Formatter, Result as FormatterResult};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::CallContractInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_cairo_utils::{deserialize_retdata, RetdataDeserializationError, TryFromIterator};
use apollo_l1_gas_price_config::config::{
    ChainlinkOracleConfig,
    DerivedRateConfig,
    FeedRead,
    MicroUnitBounds,
    MICRO_UNIT_DECIMALS,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{
    CurrencyPair,
    EthToFri,
    ExchangeRate,
    ExchangeRateOracleClientTrait,
    RateKind,
    StrkToUsd,
};
use apollo_metrics::metrics::set_unix_now_seconds;
use async_trait::async_trait;
use futures::future::try_join;
use futures::FutureExt;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;
use tokio::time::Instant;
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
    STRK_TO_USD_ORACLE_METRICS,
};

#[cfg(test)]
#[path = "chainlink_oracle_test.rs"]
mod chainlink_oracle_test;

const LATEST_ROUND_DATA_ENTRY_POINT: &str = "latest_round_data";
const DECIMALS_ENTRY_POINT: &str = "decimals";

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

/// How many sampling intervals may separate the last valid read's block timestamp from the block
/// timestamp being served, while that read is still served. Three read intervals, 45 minutes at the
/// production 900 second interval. The bound applies in both directions, because a re-proposal can
/// ask for a timestamp earlier than the read the client holds.
const MAX_FALLBACK_SAMPLING_INTERVALS: u64 = 3;

/// A rate at `RATE_DECIMALS`, or the guard trip that rejected it.
type RateResult = Result<ExchangeRate, ExchangeRateOracleClientError>;
/// A read in flight, resolving to a rate already dated by the block timestamp it was issued for.
type RateQuery = AbortOnDropHandle<Result<ValidRead, ExchangeRateOracleClientError>>;

/// A rate that passed every guard, and the block timestamp it was read for. That timestamp is a
/// block timestamp rather than a local one, because the distance a later caller measures against it
/// must come out the same on every node, including on a replay of the same block.
#[derive(Clone, Copy, Debug)]
struct ValidRead {
    rate: ExchangeRate,
    block_timestamp: u64,
}

#[derive(Default)]
struct OracleState {
    /// The newest read that passed every guard, served to callers while it is within
    /// `MAX_FALLBACK_SAMPLING_INTERVALS` of their own block timestamp.
    last_valid_read: Option<ValidRead>,
    /// The newest query's failure, cleared by the next success. Served only when no valid read is
    /// close enough to the caller.
    last_error: Option<ExchangeRateOracleClientError>,
    /// The query in flight. A single slot bounds this client to one query at a time.
    query: Option<RateQuery>,
    /// When the last query was spawned, on the local monotonic clock, which the refresh cadence is
    /// measured from. Local because the cadence is this node's own scheduling, so a block
    /// timestamp arriving from the network cannot steer it.
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
#[derive(Clone)]
pub struct ChainlinkOracleClient<Kind: ChainlinkRate> {
    config: ChainlinkOracleConfig,
    /// How often a healthy feed is read, however many proposals that spans.
    sampling_interval_seconds: NonZeroU64,
    batcher_client: SharedBatcherClient,
    state: Arc<Mutex<OracleState>>,
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
            .field("sampling_interval_seconds", &self.sampling_interval_seconds)
            .finish_non_exhaustive()
    }
}

impl<Kind: ChainlinkRate> ChainlinkOracleClient<Kind> {
    /// `sampling_interval_seconds` is the feed's own sampling cadence, taken from the same config
    /// key the HTTP client of that feed samples on, so switching source leaves the cadence
    /// unchanged.
    pub fn new(
        config: ChainlinkOracleConfig,
        sampling_interval_seconds: NonZeroU64,
        batcher_client: SharedBatcherClient,
    ) -> Self {
        let pair = Kind::PAIR;
        info!(
            "Creating ChainlinkOracleClient for {pair:?} with: \
             sampling_interval_seconds={sampling_interval_seconds} config {config:?}"
        );
        let metrics = Kind::metrics();
        metrics.register();
        register_chainlink_guard_metrics();
        let state = Arc::new(Mutex::new(OracleState::default()));
        Self {
            config,
            sampling_interval_seconds,
            batcher_client,
            state,
            metrics,
            _kind: PhantomData,
        }
    }

    /// `block_timestamp` is what every freshness guard inside the query is measured against, and
    /// what the resulting read is dated by.
    fn spawn_query(&self, block_timestamp: u64) -> RateQuery {
        let batcher_client = self.batcher_client.clone();
        let config = self.config.clone();
        let metrics = self.metrics;
        let pair = Kind::PAIR;
        AbortOnDropHandle::new(tokio::spawn(async move {
            let result = Kind::query_rate(&batcher_client, &config, block_timestamp).await;
            match &result {
                Ok(rate) => {
                    metrics.success_count.increment(1);
                    set_unix_now_seconds(metrics.last_success_timestamp);
                    metrics.rate.set_lossy(*rate);
                    debug!(
                        "Resolved {pair:?} query for block timestamp {block_timestamp} to {rate}"
                    );
                }
                Err(error) => {
                    metrics.error_count.increment(1);
                    warn!("Failed {pair:?} query for block timestamp {block_timestamp}: {error:?}");
                }
            }
            result.map(|rate| ValidRead { rate, block_timestamp })
        }))
    }

    /// Moves a finished query's outcome into `state`: a success becomes the last valid read and
    /// clears the last error, a failure becomes the last error. Called on every `fetch_rate`, so
    /// that a query which resolved after the last caller that could have observed it is harvested
    /// rather than dropped together with the round trip that produced it.
    fn harvest_finished_query(&self, state: &mut OracleState) {
        if !state.query.as_ref().is_some_and(|query| query.is_finished()) {
            return;
        }
        let joined = state
            .query
            .take()
            .expect("Query must be present if it reported being finished")
            .now_or_never()
            .expect("Finished query must resolve immediately");
        let result = joined.unwrap_or_else(|error| {
            self.metrics.error_count.increment(1);
            warn!("Query failed to join its handle: {error:?}");
            Err(ExchangeRateOracleClientError::JoinError(error.to_string()))
        });
        match result {
            Ok(valid_read) => {
                debug!("Harvested {valid_read:?}");
                state.last_valid_read = Some(valid_read);
                state.last_error = None;
            }
            Err(error) => {
                debug!("Harvested query failure: {error:?}");
                state.last_error = Some(error);
            }
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
            self.sampling_interval_seconds.get()
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
            MAX_FALLBACK_SAMPLING_INTERVALS.saturating_mul(self.sampling_interval_seconds.get());
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
        block_timestamp: u64,
    ) -> RateResult {
        read_feed(batcher_client, config.strk_usd_feed(), block_timestamp).await
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
        block_timestamp: u64,
    ) -> RateResult {
        // The two legs are separate `call_contract` calls, which exposes no block pinning, so
        // they may straddle a block boundary. A one-block skew is orders of magnitude below
        // the staleness bound both legs must independently pass.
        let (eth_to_usd_rate, strk_to_usd_rate) = try_join(
            read_feed(batcher_client, config.eth_usd_feed(), block_timestamp),
            read_feed(batcher_client, config.strk_usd_feed(), block_timestamp),
        )
        .await?;

        let eth_to_fri_rate = derive_eth_to_fri_rate(eth_to_usd_rate, strk_to_usd_rate)?;
        config.eth_to_fri.check_rate(eth_to_fri_rate)?;
        Ok(eth_to_fri_rate)
    }
}

/// Bounds the derived rate against its own pair's band, so the pair is not a separate argument.
trait CheckDerivedRate {
    fn check_rate(&self, rate: ExchangeRate) -> Result<(), ExchangeRateOracleClientError>;
}

impl CheckDerivedRate for DerivedRateConfig {
    fn check_rate(&self, rate: ExchangeRate) -> Result<(), ExchangeRateOracleClientError> {
        check_rate_bounds(rate, self, CurrencyPair::EthStrk)
    }
}

/// The feed's answer, rescaled to `RATE_DECIMALS` and checked against the feed's bounds.
async fn read_feed(
    batcher_client: &SharedBatcherClient,
    feed: FeedRead<'_>,
    block_timestamp: u64,
) -> RateResult {
    let pair = feed.pair;
    let pair_name = pair.pair_name();
    let feed_address = feed.feed.feed_address;
    // The feed's `decimals` is read alongside every rate rather than cached, because a feed that
    // changes it would rescale the answer by a power of ten, and the absolute bounds are too wide
    // to catch that. STRK/USD accepts $0.0001 to $10, so an 8-decimal answer read as 6 decimals
    // passes as $3.00 instead of $0.03.
    let (decimals_retdata, round_retdata) = try_join(
        call_view(batcher_client, feed_address, DECIMALS_ENTRY_POINT, pair),
        call_view(batcher_client, feed_address, LATEST_ROUND_DATA_ENTRY_POINT, pair),
    )
    .await?;
    let feed_decimals = decode_feed_decimals(decimals_retdata, pair)?;

    let round: ChainlinkRoundData = decode_retdata(round_retdata, pair)?;
    if round.answer == 0 {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} returned a zero answer"
        )));
    }
    if block_timestamp.saturating_sub(round.updated_at) > feed.max_staleness_seconds {
        CHAINLINK_ORACLE_STALE_FEED_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::StaleFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            block_timestamp,
            max_staleness_seconds: feed.max_staleness_seconds,
        });
    }
    // Catches a round dated ahead of the block being priced: the staleness check above saturates
    // such a subtraction to zero, which alone treats it as fresh regardless of age.
    if round.updated_at.saturating_sub(block_timestamp) > feed.max_future_updated_at_seconds {
        CHAINLINK_ORACLE_FUTURE_FEED_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::FutureFeedError {
            pair_name: pair_name.to_string(),
            updated_at: round.updated_at,
            block_timestamp,
            max_future_updated_at_seconds: feed.max_future_updated_at_seconds,
        });
    }

    let rate = rescale_to_rate_decimals(round.answer, feed_decimals)?;
    check_rate_bounds(rate, feed.feed, pair)?;
    Ok(rate)
}

async fn call_view(
    batcher_client: &SharedBatcherClient,
    contract_address: ContractAddress,
    entry_point: &str,
    pair: CurrencyPair,
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
            CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1, &pair.labels());
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
    pair: CurrencyPair,
) -> Result<u32, ExchangeRateOracleClientError> {
    let pair_name = pair.pair_name();
    let raw_decimals: Felt = decode_retdata(decimals_retdata, pair)?;
    let feed_decimals = u32::try_from(raw_decimals).map_err(|_| {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &pair.labels());
        ExchangeRateOracleClientError::ParseError(format!(
            "{pair_name} decimals {raw_decimals} does not fit in u32"
        ))
    })?;
    if !(MIN_FEED_DECIMALS..=MAX_FEED_DECIMALS).contains(&feed_decimals) {
        CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} reports {feed_decimals} decimals, outside the accepted range \
             [{MIN_FEED_DECIMALS}, {MAX_FEED_DECIMALS}]"
        )));
    }
    Ok(feed_decimals)
}

fn decode_retdata<T>(
    retdata: Vec<Felt>,
    pair: CurrencyPair,
) -> Result<T, ExchangeRateOracleClientError>
where
    T: TryFromIterator<Felt, Error = RetdataDeserializationError>,
{
    deserialize_retdata(retdata).map_err(|error| {
        CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT.increment(1, &pair.labels());
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
fn derive_eth_to_fri_rate(
    eth_to_usd_rate: ExchangeRate,
    strk_to_usd_rate: ExchangeRate,
) -> RateResult {
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

// TODO(Asaf): bound the rate's change against the previous block's implied rate. The absolute
// bounds below are wide enough to pass a manipulated but plausible answer, the STRK/USD pair alone
// accepting anything from $0.0001 to $10, which only a bound relative to the last accepted rate
// catches. It must be anchored to the block header rather than to node-local history, so that every
// validator accepts and rejects the same values.
/// Absolute bounds are the only defense against a feed wired to the wrong asset or a
/// plausible-but-poisoned answer: consensus checks that validators agree with each other, never
/// that the agreed value is sane, and every node reads the same chain state.
fn check_rate_bounds(
    rate: ExchangeRate,
    bounds: &impl MicroUnitBounds,
    pair: CurrencyPair,
) -> Result<(), ExchangeRateOracleClientError> {
    let min_rate =
        u128::from(bounds.minimum_micro_units()).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    let max_rate =
        u128::from(bounds.maximum_micro_units()).saturating_mul(MICRO_UNIT_TO_RATE_SCALE);
    if rate < min_rate || rate > max_rate {
        CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT.increment(1, &pair.labels());
        return Err(ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair_name: pair.pair_name().to_string(),
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
