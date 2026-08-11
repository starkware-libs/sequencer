use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use apollo_batcher_types::batcher_types::CallContractOutput;
use apollo_batcher_types::communication::{BatcherClientError, MockBatcherClient};
use apollo_batcher_types::errors::BatcherError;
use apollo_metrics::metrics::{LabeledMetricCounter, MetricDetails};
use assert_matches::assert_matches;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use strum::IntoEnumIterator;

use super::*;

const TIMESTAMP: u64 = 1_700_000_000;
const FEED_DECIMALS: u32 = 8;
const MAX_POLL_ATTEMPTS: usize = 1000;
const MICRO_UNITS_PER_UNIT: u64 = 1_000_000;
/// `decimals` and `latest_round_data`, per feed.
const CALLS_PER_FEED_PER_BUCKET: usize = 2;

/// $3000 per ETH at `FEED_DECIMALS`.
const ETH_USD_ANSWER: u128 = 300_000_000_000;
/// $0.03 per STRK at `FEED_DECIMALS`.
const STRK_USD_ANSWER: u128 = 3_000_000;
/// The rate the two answers above derive to: 100,000 STRK per ETH.
const ETH_TO_FRI_RATE: u128 = 100_000 * RATE_SCALE;
/// $0.03 per STRK at `RATE_DECIMALS`.
const STRK_TO_USD_RATE: u128 = 30_000_000_000_000_000;

type FeedResponses = HashMap<(ContractAddress, String), Vec<Felt>>;

fn test_config() -> ChainlinkOracleConfig {
    ChainlinkOracleConfig::default()
}

/// The timestamp the client actually queries for, after quantization.
fn quantized_query_timestamp(timestamp: u64) -> u64 {
    let lag_interval_seconds = test_config().lag_interval_seconds;
    (timestamp - lag_interval_seconds) / lag_interval_seconds * lag_interval_seconds
}

fn fresh_updated_at() -> u64 {
    quantized_query_timestamp(TIMESTAMP)
}

fn stale_updated_at() -> u64 {
    quantized_query_timestamp(TIMESTAMP) - test_config().max_staleness_seconds - 1
}

fn future_updated_at() -> u64 {
    quantized_query_timestamp(TIMESTAMP) + test_config().max_future_updated_at_seconds + 1
}

/// One feed's mocked `decimals` and `latest_round_data` replies.
#[derive(Clone, Copy)]
struct FeedFixture {
    answer: u128,
    updated_at: u64,
    feed_decimals: u32,
}

impl FeedFixture {
    fn new(answer: u128, updated_at: u64) -> Self {
        Self { answer, updated_at, feed_decimals: FEED_DECIMALS }
    }

    fn with_decimals(self, feed_decimals: u32) -> Self {
        Self { feed_decimals, ..self }
    }
}

fn decimals_retdata(feed_decimals: u32) -> Vec<Felt> {
    vec![Felt::from(feed_decimals)]
}

fn round_retdata(answer: u128, updated_at: u64) -> Vec<Felt> {
    // A realistic phase-encoded `round_id`: `(phase_id << 128) | aggregator_round_id`, which
    // exceeds u64.
    const PHASE_ENCODED_ROUND_ID: &str = "0x100000000000000000000000000000042";
    const BLOCK_NUMBER: u64 = 987_654;
    const STARTED_AT: u64 = 1_699_999_000;
    vec![
        Felt::from_hex_unchecked(PHASE_ENCODED_ROUND_ID),
        Felt::from(answer),
        Felt::from(BLOCK_NUMBER),
        Felt::from(STARTED_AT),
        Felt::from(updated_at),
    ]
}

fn feed_responses(feed_address: ContractAddress, feed: FeedFixture) -> FeedResponses {
    HashMap::from([
        ((feed_address, DECIMALS_ENTRY_POINT.to_string()), decimals_retdata(feed.feed_decimals)),
        (
            (feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            round_retdata(feed.answer, feed.updated_at),
        ),
    ])
}

fn strk_usd_responses(strk_usd: FeedFixture) -> FeedResponses {
    feed_responses(test_config().strk_usd_feed_address, strk_usd)
}

/// The STRK/USD feed with a valid `decimals` reply but no round, so exactly one of the two calls
/// the client issues per feed fails.
fn strk_usd_responses_without_round_data() -> FeedResponses {
    HashMap::from([(
        (test_config().strk_usd_feed_address, DECIMALS_ENTRY_POINT.to_string()),
        decimals_retdata(FEED_DECIMALS),
    )])
}

fn eth_and_strk_responses(eth_usd: FeedFixture, strk_usd: FeedFixture) -> FeedResponses {
    let mut responses = feed_responses(test_config().eth_usd_feed_address, eth_usd);
    responses.extend(feed_responses(test_config().strk_usd_feed_address, strk_usd));
    responses
}

fn counting_batcher_client(responses: FeedResponses) -> (SharedBatcherClient, Arc<AtomicUsize>) {
    let num_calls = Arc::new(AtomicUsize::new(0));
    let num_calls_in_mock = num_calls.clone();
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(move |input| {
        num_calls_in_mock.fetch_add(1, Ordering::SeqCst);
        responses
            .get(&(input.contract_address, input.entry_point.clone()))
            .map(|retdata| CallContractOutput { retdata: retdata.clone() })
            .ok_or(BatcherClientError::BatcherError(BatcherError::InternalError))
    });
    (Arc::new(batcher_client), num_calls)
}

/// Serves `responses` for the first `num_served_calls` calls and fails every call after that,
/// which lets a test hold a good bucket followed by a broken one.
fn batcher_client_failing_after(
    responses: FeedResponses,
    num_served_calls: usize,
) -> SharedBatcherClient {
    let num_calls = AtomicUsize::new(0);
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(move |input| {
        if num_calls.fetch_add(1, Ordering::SeqCst) >= num_served_calls {
            return Err(BatcherClientError::BatcherError(BatcherError::InternalError));
        }
        responses
            .get(&(input.contract_address, input.entry_point.clone()))
            .map(|retdata| CallContractOutput { retdata: retdata.clone() })
            .ok_or(BatcherClientError::BatcherError(BatcherError::InternalError))
    });
    Arc::new(batcher_client)
}

fn batcher_client_from_responses(responses: FeedResponses) -> SharedBatcherClient {
    counting_batcher_client(responses).0
}

fn make_client(rate_kind: ChainlinkRateKind, responses: FeedResponses) -> ChainlinkOracleClient {
    ChainlinkOracleClient::new(rate_kind, test_config(), batcher_client_from_responses(responses))
}

/// Polls until the spawned background query resolves, mirroring how consensus retries across
/// proposals.
async fn resolve_rate(
    client: &ChainlinkOracleClient,
    timestamp: u64,
) -> Result<u128, ExchangeRateOracleClientError> {
    for _ in 0..MAX_POLL_ATTEMPTS {
        match client.fetch_rate(timestamp).await {
            Err(ExchangeRateOracleClientError::QueryNotReadyError(_)) => {
                tokio::task::yield_now().await;
            }
            resolved => return resolved,
        }
    }
    panic!("Query did not resolve within {MAX_POLL_ATTEMPTS} attempts");
}

/// The result the client holds for the bucket `timestamp` falls into, once that bucket's query has
/// resolved.
fn bucket_result(
    client: &ChainlinkOracleClient,
    timestamp: u64,
) -> Option<Result<u128, ExchangeRateOracleClientError>> {
    let state = client.state.lock().unwrap();
    let bucket = state.current_bucket.as_ref()?;
    if bucket.query_timestamp != quantized_query_timestamp(timestamp) {
        return None;
    }
    bucket.result.clone()
}

fn last_valid_rate(client: &ChainlinkOracleClient) -> Option<u128> {
    client.state.lock().unwrap().last_valid_read.map(|valid_read| valid_read.rate)
}

/// Drives `fetch_rate` until the bucket has a result of its own. Unlike `resolve_rate` this does
/// not stop at the first `Ok`, which for a failing bucket is the last valid rate.
async fn wait_for_bucket_result(client: &ChainlinkOracleClient, timestamp: u64) {
    for _ in 0..MAX_POLL_ATTEMPTS {
        if bucket_result(client, timestamp).is_some() {
            return;
        }
        let _ = client.fetch_rate(timestamp).await;
        tokio::task::yield_now().await;
    }
    panic!("Query for timestamp {timestamp} did not resolve within {MAX_POLL_ATTEMPTS} attempts");
}

fn micro_units_to_answer(micro_units: u64, feed_decimals: u32) -> u128 {
    u128::from(micro_units) * 10u128.pow(feed_decimals - MICRO_UNIT_DECIMALS)
}

fn micro_units_to_rate(micro_units: u64) -> u128 {
    u128::from(micro_units) * MICRO_UNIT_TO_RATE_SCALE
}

#[tokio::test]
async fn strk_to_usd_rescales_feed_answer_to_eighteen_decimals() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
    );
    // $0.03 per STRK.
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), STRK_TO_USD_RATE);
}

#[tokio::test]
async fn eth_to_fri_divides_the_two_usd_legs() {
    let updated_at = fresh_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(ETH_USD_ANSWER, updated_at),
            FeedFixture::new(STRK_USD_ANSWER, updated_at),
        ),
    );
    // $3000 per ETH over $0.03 per STRK is 100,000 STRK per ETH.
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), ETH_TO_FRI_RATE);
}

/// Each answer is rescaled to `RATE_DECIMALS` before the division, so the derived rate must come
/// out the same whatever scales the two feeds report at. The widest pairs also cover the rescale
/// of the largest answer this client accepts without overflowing.
#[rstest]
#[case::equal_decimals(8, 8)]
#[case::wider_strk_feed(8, 12)]
#[case::widest_strk_feed(6, 18)]
#[case::widest_eth_feed(18, 6)]
#[tokio::test]
async fn eth_to_fri_is_independent_of_the_feeds_decimals(
    #[case] eth_usd_decimals: u32,
    #[case] strk_usd_decimals: u32,
) {
    let updated_at = fresh_updated_at();
    // $3000 per ETH and $0.03 per STRK, expressed at each feed's own scale.
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(3000 * 10u128.pow(eth_usd_decimals), updated_at)
                .with_decimals(eth_usd_decimals),
            FeedFixture::new(3 * 10u128.pow(strk_usd_decimals) / 100, updated_at)
                .with_decimals(strk_usd_decimals),
        ),
    );
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), ETH_TO_FRI_RATE);
}

/// The two legs do not divide evenly here, so this is the case that exercises recombining the
/// scaled quotient with the scaled remainder, the one step of the derivation whose result cannot
/// be read off the inputs.
#[tokio::test]
async fn eth_to_fri_recombines_a_non_zero_remainder() {
    /// $0.07 per STRK at `FEED_DECIMALS`.
    const STRK_USD_ANSWER_SEVEN_CENTS: u128 = 7_000_000;
    /// floor(3000 / 0.07 * 10^18), that is 42857.142857... STRK per ETH.
    const EXPECTED_RATE: u128 = 42_857_142_857_142_857_142_857;
    let updated_at = fresh_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(ETH_USD_ANSWER, updated_at),
            FeedFixture::new(STRK_USD_ANSWER_SEVEN_CENTS, updated_at),
        ),
    );
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), EXPECTED_RATE);
}

#[tokio::test]
async fn strk_to_usd_rejects_stale_reading() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())),
    );
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair_name, .. })
            if pair_name == STRK_USD_PAIR_NAME
    );
}

#[tokio::test]
async fn strk_to_usd_accepts_reading_exactly_at_the_staleness_bound() {
    let config = test_config();
    let oldest_accepted_updated_at =
        quantized_query_timestamp(TIMESTAMP) - config.max_staleness_seconds;
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, oldest_accepted_updated_at)),
    );
    assert!(resolve_rate(&client, TIMESTAMP).await.is_ok());
}

/// The future-dated bound, checked separately from the staleness bound (see `read_feed`).
#[rstest]
#[case::at_the_future_bound(0, true)]
#[case::just_past_the_future_bound(1, false)]
#[tokio::test]
async fn future_updated_at_is_bounded(
    #[case] seconds_past_the_bound: u64,
    #[case] is_accepted: bool,
) {
    let config = test_config();
    let updated_at = quantized_query_timestamp(TIMESTAMP)
        + config.max_future_updated_at_seconds
        + seconds_past_the_bound;
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, updated_at)),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::FutureFeedError { pair_name, .. })
                if pair_name == STRK_USD_PAIR_NAME
        );
    }
}

/// `u64::MAX` as `updated_at` is rejected by the future bound (see `read_feed`).
#[tokio::test]
async fn maximal_future_updated_at_rejected() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, u64::MAX)),
    );
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::FutureFeedError { .. })
    );
}

/// A fresh leg divided by a stale leg produces a phantom rate move, so either stale leg must
/// reject the whole derived rate.
#[rstest]
#[case::stale_eth_leg(true, false, ETH_USD_PAIR_NAME)]
#[case::stale_strk_leg(false, true, STRK_USD_PAIR_NAME)]
#[tokio::test]
async fn eth_to_fri_rejects_when_either_leg_is_stale(
    #[case] is_eth_leg_stale: bool,
    #[case] is_strk_leg_stale: bool,
    #[case] expected_pair_name: &str,
) {
    let fresh_timestamp = fresh_updated_at();
    let stale_timestamp = stale_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(
                ETH_USD_ANSWER,
                if is_eth_leg_stale { stale_timestamp } else { fresh_timestamp },
            ),
            FeedFixture::new(
                STRK_USD_ANSWER,
                if is_strk_leg_stale { stale_timestamp } else { fresh_timestamp },
            ),
        ),
    );
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair_name, .. })
            if pair_name == expected_pair_name
    );
}

#[tokio::test]
async fn zero_answer_rejected() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(0, fresh_updated_at())),
    );
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::InvalidRateError(message))
            if message.contains("zero answer")
    );
}

/// The accepted range is what bounds the rescale; a feed reporting outside it is rejected rather
/// than mis-scaled.
#[rstest]
#[case::at_the_minimum(MIN_FEED_DECIMALS, true)]
#[case::at_the_maximum(MAX_FEED_DECIMALS, true)]
#[case::below_the_minimum(MIN_FEED_DECIMALS - 1, false)]
#[case::above_the_maximum(MAX_FEED_DECIMALS + 1, false)]
#[tokio::test]
async fn feed_decimals_range_is_enforced(#[case] feed_decimals: u32, #[case] is_accepted: bool) {
    // $0.03 per STRK, expressed at the feed's own scale.
    let answer = 3 * 10u128.pow(feed_decimals) / 100;
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(
            FeedFixture::new(answer, fresh_updated_at()).with_decimals(feed_decimals),
        ),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(result.unwrap(), STRK_TO_USD_RATE);
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::InvalidRateError(message))
                if message.contains("decimals")
        );
    }
}

/// A decimals value too large for a `u32` must be reported as a parse failure rather than
/// truncated into a plausible scale.
#[tokio::test]
async fn feed_decimals_exceeding_u32_rejected() {
    let strk_usd_feed_address = test_config().strk_usd_feed_address;
    let responses = HashMap::from([
        ((strk_usd_feed_address, DECIMALS_ENTRY_POINT.to_string()), vec![Felt::MAX]),
        (
            (strk_usd_feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            round_retdata(STRK_USD_ANSWER, fresh_updated_at()),
        ),
    ]);
    let client = make_client(ChainlinkRateKind::StrkToUsd, responses);
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ParseError(message)) if message.contains("u32")
    );
}

#[rstest]
#[case::at_minimum(0, true)]
#[case::just_below_minimum(-1, false)]
#[tokio::test]
async fn strk_usd_minimum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer =
        micro_units_to_answer(config.strk_usd_price_bounds.minimum_micro_units, FEED_DECIMALS);
    let answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(answer, fresh_updated_at())),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(
            result.unwrap(),
            micro_units_to_rate(config.strk_usd_price_bounds.minimum_micro_units)
        );
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == STRK_USD_PAIR_NAME
        );
    }
}

#[rstest]
#[case::at_maximum(0, true)]
#[case::just_above_maximum(1, false)]
#[tokio::test]
async fn strk_usd_maximum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer =
        micro_units_to_answer(config.strk_usd_price_bounds.maximum_micro_units, FEED_DECIMALS);
    let answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(answer, fresh_updated_at())),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(
            result.unwrap(),
            micro_units_to_rate(config.strk_usd_price_bounds.maximum_micro_units)
        );
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == STRK_USD_PAIR_NAME
        );
    }
}

#[rstest]
#[case::at_minimum(0, true)]
#[case::just_below_minimum(-1, false)]
#[tokio::test]
async fn eth_usd_minimum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer =
        micro_units_to_answer(config.eth_usd_price_bounds.minimum_micro_units, FEED_DECIMALS);
    let eth_usd_answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    // $0.001 per STRK keeps the derived rate at 20,000 STRK per ETH, inside its own bounds, so
    // only the ETH/USD bound can trip.
    const STRK_USD_ANSWER_TENTH_OF_A_CENT: u128 = 100_000;
    let updated_at = fresh_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(eth_usd_answer, updated_at),
            FeedFixture::new(STRK_USD_ANSWER_TENTH_OF_A_CENT, updated_at),
        ),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == ETH_USD_PAIR_NAME
        );
    }
}

#[rstest]
#[case::at_maximum(0, true)]
#[case::just_above_maximum(1, false)]
#[tokio::test]
async fn eth_usd_maximum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer =
        micro_units_to_answer(config.eth_usd_price_bounds.maximum_micro_units, FEED_DECIMALS);
    let eth_usd_answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    // $0.50 per STRK keeps the derived rate at 100,000 STRK per ETH, inside its own bounds.
    const STRK_USD_ANSWER_FIFTY_CENTS: u128 = 50_000_000;
    let updated_at = fresh_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(eth_usd_answer, updated_at),
            FeedFixture::new(STRK_USD_ANSWER_FIFTY_CENTS, updated_at),
        ),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == ETH_USD_PAIR_NAME
        );
    }
}

/// Which end of the derived ETH/STRK band a case lands on.
#[derive(Clone, Copy)]
enum DerivedRateBound {
    Minimum,
    Maximum,
}

/// An ETH price well inside the ETH/USD band. The STRK price is derived from it so that the
/// derived rate lands exactly on the requested ETH/STRK bound.
const ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD: u64 = 10_000 * MICRO_UNITS_PER_UNIT;

fn strk_usd_answer_for_derived_rate(rate_micro_strk: u64) -> u128 {
    // The rate is the ETH price over the STRK price, so the STRK price that produces it is the
    // ETH price over the rate, with one extra micro-unit factor to keep both in micro units.
    let strk_usd_price_micro_usd =
        ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD * MICRO_UNITS_PER_UNIT / rate_micro_strk;
    micro_units_to_answer(strk_usd_price_micro_usd, FEED_DECIMALS)
}

/// Both legs stay inside their own bounds; only the derived rate crosses its bound. The rate moves
/// inversely with the STRK price, which is why the offset that breaks the floor is positive and
/// the one that breaks the ceiling is negative.
#[rstest]
#[case::at_minimum(DerivedRateBound::Minimum, 0, true)]
#[case::just_below_minimum(DerivedRateBound::Minimum, 1, false)]
#[case::at_maximum(DerivedRateBound::Maximum, 0, true)]
#[case::just_above_maximum(DerivedRateBound::Maximum, -1, false)]
#[tokio::test]
async fn eth_to_fri_bounds_are_inclusive(
    #[case] bound: DerivedRateBound,
    #[case] strk_usd_answer_offset: i8,
    #[case] is_accepted: bool,
) {
    let config = test_config();
    let boundary_rate_micro_strk = match bound {
        DerivedRateBound::Minimum => config.eth_to_fri_rate_bounds.minimum_micro_units,
        DerivedRateBound::Maximum => config.eth_to_fri_rate_bounds.maximum_micro_units,
    };
    let strk_usd_answer = strk_usd_answer_for_derived_rate(boundary_rate_micro_strk)
        .wrapping_add_signed(strk_usd_answer_offset.into());
    let updated_at = fresh_updated_at();
    let client = make_client(
        ChainlinkRateKind::EthToFri,
        eth_and_strk_responses(
            FeedFixture::new(
                micro_units_to_answer(ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD, FEED_DECIMALS),
                updated_at,
            ),
            FeedFixture::new(strk_usd_answer, updated_at),
        ),
    );

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(result.unwrap(), micro_units_to_rate(boundary_rate_micro_strk));
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == ETH_STRK_PAIR_NAME
        );
    }
}

#[tokio::test]
async fn extreme_answer_errors_instead_of_overflowing() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(u128::MAX, fresh_updated_at())),
    );
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ArithmeticError(_))
    );
}

#[rstest]
#[case::too_few_felts(round_retdata(STRK_USD_ANSWER, 0).into_iter().take(4).collect())]
#[case::too_many_felts(
    [round_retdata(STRK_USD_ANSWER, 0), vec![Felt::ONE]].concat()
)]
#[case::answer_exceeding_u128(vec![Felt::ONE, Felt::MAX, Felt::ONE, Felt::ONE, Felt::ONE])]
#[case::updated_at_exceeding_u64(
    vec![Felt::ONE, Felt::from(STRK_USD_ANSWER), Felt::ONE, Felt::ONE, Felt::MAX]
)]
#[tokio::test]
async fn malformed_retdata_rejected(#[case] malformed_round_retdata: Vec<Felt>) {
    let strk_usd_feed_address = test_config().strk_usd_feed_address;
    let responses = HashMap::from([
        (
            (strk_usd_feed_address, DECIMALS_ENTRY_POINT.to_string()),
            decimals_retdata(FEED_DECIMALS),
        ),
        (
            (strk_usd_feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            malformed_round_retdata,
        ),
    ]);
    let client = make_client(ChainlinkRateKind::StrkToUsd, responses);
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ParseError(_))
    );
}

#[tokio::test]
async fn first_call_spawns_a_query_and_later_calls_hit_the_cache() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::new(ChainlinkRateKind::StrkToUsd, test_config(), batcher_client);

    // The batcher round trip must never block the proposal path.
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );

    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();
    let num_calls_after_resolution = num_batcher_calls.load(Ordering::SeqCst);

    // A later timestamp in the same bucket is served from the cache.
    let same_bucket_timestamp = TIMESTAMP + 1;
    assert_eq!(
        quantized_query_timestamp(same_bucket_timestamp),
        quantized_query_timestamp(TIMESTAMP)
    );
    assert_eq!(client.fetch_rate(same_bucket_timestamp).await.unwrap(), rate);
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), num_calls_after_resolution);
}

/// While the next bucket's query is still in flight, the last valid rate is served rather than
/// blocking the proposal path.
#[tokio::test]
async fn unresolved_bucket_falls_back_to_the_last_valid_rate() {
    let client = make_client(
        ChainlinkRateKind::StrkToUsd,
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    let next_bucket_timestamp = TIMESTAMP + test_config().lag_interval_seconds;
    assert_eq!(client.fetch_rate(next_bucket_timestamp).await.unwrap(), rate);
}

/// A held failure keeps the batcher from being queried again this bucket, but it must not deny the
/// proposal path a rate the client already holds.
#[tokio::test]
async fn held_failure_still_falls_back_to_the_last_valid_rate() {
    let client = ChainlinkOracleClient::new(
        ChainlinkRateKind::StrkToUsd,
        test_config(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_BUCKET,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    let next_bucket_timestamp = TIMESTAMP + test_config().lag_interval_seconds;
    wait_for_bucket_result(&client, next_bucket_timestamp).await;
    assert_matches!(bucket_result(&client, next_bucket_timestamp), Some(Err(_)));

    assert_eq!(client.fetch_rate(next_bucket_timestamp).await.unwrap(), rate);
}

/// The fallback must hold for the one call that observes a failing query complete and stores the
/// failure, not just the calls before and after it.
#[tokio::test]
async fn no_call_is_denied_a_rate_the_client_holds() {
    let client = ChainlinkOracleClient::new(
        ChainlinkRateKind::StrkToUsd,
        test_config(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_BUCKET,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    let next_bucket_timestamp = TIMESTAMP + test_config().lag_interval_seconds;
    for _ in 0..MAX_POLL_ATTEMPTS {
        if bucket_result(&client, next_bucket_timestamp).is_some() {
            break;
        }
        assert_eq!(
            client
                .fetch_rate(next_bucket_timestamp)
                .await
                .expect("Last valid rate should be served"),
            rate
        );
        tokio::task::yield_now().await;
    }
    assert_matches!(bucket_result(&client, next_bucket_timestamp), Some(Err(_)));
}

/// `decimals` and `latest_round_data` are read once per bucket, however many proposals that bucket
/// spans.
#[tokio::test]
async fn each_bucket_queries_the_feed_once() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::new(ChainlinkRateKind::StrkToUsd, test_config(), batcher_client);

    resolve_rate(&client, TIMESTAMP).await.unwrap();
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_BUCKET);

    let later_timestamp = TIMESTAMP + test_config().lag_interval_seconds;
    // The fallback serves this bucket while its own query runs, so wait for the query rather than
    // for a rate.
    wait_for_bucket_result(&client, later_timestamp).await;
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 2 * CALLS_PER_FEED_PER_BUCKET);

    const NUM_LATER_CALLS: usize = 10;
    for _ in 0..NUM_LATER_CALLS {
        client.fetch_rate(later_timestamp).await.unwrap();
    }
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 2 * CALLS_PER_FEED_PER_BUCKET);
}

#[tokio::test]
async fn batcher_call_failure_is_surfaced_as_an_error() {
    let client = make_client(ChainlinkRateKind::StrkToUsd, FeedResponses::new());
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
}

/// With no valid rate to fall back on, the caller sees the failure, and the failing feed is
/// queried once for the bucket rather than once per call.
#[tokio::test]
async fn failed_query_is_held_for_the_rest_of_the_bucket() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(FeedResponses::new());
    let client =
        ChainlinkOracleClient::new(ChainlinkRateKind::StrkToUsd, test_config(), batcher_client);

    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
    let num_calls_after_failure = num_batcher_calls.load(Ordering::SeqCst);
    assert_eq!(last_valid_rate(&client), None);

    // Every offset stays inside the bucket `TIMESTAMP` falls into, asserted below.
    const NUM_LATER_CALLS: u64 = 10;
    for call_offset in 1..=NUM_LATER_CALLS {
        let same_bucket_timestamp = TIMESTAMP + call_offset;
        assert_eq!(
            quantized_query_timestamp(same_bucket_timestamp),
            quantized_query_timestamp(TIMESTAMP)
        );
        assert_matches!(
            client.fetch_rate(same_bucket_timestamp).await,
            Err(ExchangeRateOracleClientError::ContractCallError(_))
        );
        tokio::task::yield_now().await;
    }
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), num_calls_after_failure);
}

/// The last valid rate is served while it is within `MAX_FALLBACK_LAG_INTERVALS` of the bucket
/// being queried, however many buckets passed without a call.
#[rstest]
#[case::at_the_allowance(MAX_FALLBACK_LAG_INTERVALS, true)]
#[case::one_interval_past_the_allowance(MAX_FALLBACK_LAG_INTERVALS + 1, false)]
#[case::an_hour_of_buckets_later(60, false)]
#[tokio::test]
async fn last_valid_rate_is_served_only_within_the_allowance(
    #[case] num_intervals_ahead: u64,
    #[case] is_served: bool,
) {
    let client = ChainlinkOracleClient::new(
        ChainlinkRateKind::StrkToUsd,
        test_config(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_BUCKET,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    // No call is made in between, so the buckets the rate ages by are never queried.
    let later_timestamp = TIMESTAMP + num_intervals_ahead * test_config().lag_interval_seconds;
    wait_for_bucket_result(&client, later_timestamp).await;

    let result = client.fetch_rate(later_timestamp).await;
    if is_served {
        assert_eq!(result.unwrap(), rate);
    } else {
        assert_matches!(result, Err(ExchangeRateOracleClientError::ContractCallError(_)));
    }
}

/// A query still in flight when a later bucket starts is aborted, so it never reaches the feed.
#[tokio::test]
async fn a_superseded_query_is_aborted() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::new(ChainlinkRateKind::StrkToUsd, test_config(), batcher_client);

    // `fetch_rate` never awaits, so on the test's single-threaded runtime the query it spawns
    // cannot run before the next call supersedes it.
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 0, "the query ran before it was aborted");

    let next_bucket_timestamp = TIMESTAMP + test_config().lag_interval_seconds;
    assert_matches!(
        client.fetch_rate(next_bucket_timestamp).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );

    resolve_rate(&client, next_bucket_timestamp).await.unwrap();
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_BUCKET);
}

/// Both legs stay inside their own bands; only the derived rate crosses its floor. The rate moves
/// inversely with the STRK price, so raising that price by one unit is what breaks the floor.
fn responses_with_derived_rate_below_floor() -> FeedResponses {
    let updated_at = fresh_updated_at();
    let strk_usd_answer =
        strk_usd_answer_for_derived_rate(test_config().eth_to_fri_rate_bounds.minimum_micro_units)
            + 1;
    eth_and_strk_responses(
        FeedFixture::new(
            micro_units_to_answer(ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD, FEED_DECIMALS),
            updated_at,
        ),
        FeedFixture::new(strk_usd_answer, updated_at),
    )
}

/// The guard counters record why a query was rejected and, through the `feed` label, which reading
/// it was rejected on. Each guard must increment its own counter on the rejected feed's series and
/// on no other, so that a stale ETH/USD leg is distinguishable from a stale STRK/USD leg.
#[rstest]
#[case::stale_strk_feed(
    ChainlinkRateKind::StrkToUsd,
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ChainlinkFeed::StrkUsd
)]
#[case::stale_eth_leg(
    ChainlinkRateKind::EthToFri,
    eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, stale_updated_at()),
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ChainlinkFeed::EthUsd
)]
#[case::stale_strk_leg(
    ChainlinkRateKind::EthToFri,
    eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, fresh_updated_at()),
        FeedFixture::new(STRK_USD_ANSWER, stale_updated_at()),
    ),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    ChainlinkFeed::StrkUsd
)]
#[case::future_feed(
    ChainlinkRateKind::StrkToUsd,
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, future_updated_at())),
    &CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    ChainlinkFeed::StrkUsd
)]
#[case::zero_answer(
    ChainlinkRateKind::StrkToUsd,
    strk_usd_responses(FeedFixture::new(0, fresh_updated_at())),
    &CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    ChainlinkFeed::StrkUsd
)]
// One micro-cent per STRK, far below the configured floor.
#[case::rate_out_of_bounds(
    ChainlinkRateKind::StrkToUsd,
    strk_usd_responses(FeedFixture::new(1, fresh_updated_at())),
    &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    ChainlinkFeed::StrkUsd
)]
#[case::derived_rate_out_of_bounds(
    ChainlinkRateKind::EthToFri,
    responses_with_derived_rate_below_floor(),
    &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    ChainlinkFeed::EthStrk
)]
#[case::contract_call_failure(
    ChainlinkRateKind::StrkToUsd,
    strk_usd_responses_without_round_data(),
    &CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    ChainlinkFeed::StrkUsd
)]
#[tokio::test]
async fn guard_counters_record_the_rejection_reason_and_feed(
    #[case] rate_kind: ChainlinkRateKind,
    #[case] responses: FeedResponses,
    #[case] guard_counter: &'static LabeledMetricCounter,
    #[case] rejected_feed: ChainlinkFeed,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    let client = make_client(rate_kind, responses);

    assert!(resolve_rate(&client, TIMESTAMP).await.is_err());
    let rendered_metrics = recorder.handle().render();
    for feed in ChainlinkFeed::iter() {
        // A series only the guard's own increment creates reads as absent, which is the same
        // statement as a count of zero.
        let count = guard_counter
            .parse_numeric_metric::<u64>(&rendered_metrics, &feed.labels())
            .unwrap_or(0);
        let expected_count = u64::from(feed == rejected_feed);
        assert_eq!(
            count,
            expected_count,
            "{} on feed {feed:?} recorded {count} rejections, expected {expected_count}",
            guard_counter.get_name()
        );
    }
}

#[rstest]
#[case::ascii("a plain revert reason".to_string())]
#[case::multibyte("שלום".repeat(10))]
fn short_contract_call_error_is_relayed_verbatim(#[case] error_text: String) {
    assert!(error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    assert_eq!(truncate_contract_call_error(error_text.clone()), error_text);
}

/// The cap counts bytes, so a multi-byte reason must be cut at a character boundary at or just
/// below it, never mid-character.
#[rstest]
#[case::single_byte_characters("a")]
#[case::four_byte_characters("😀")]
fn long_contract_call_error_is_truncated_on_a_character_boundary(#[case] repeated_text: &str) {
    const NUM_REPETITIONS: usize = 1000;
    let error_text = repeated_text.repeat(NUM_REPETITIONS);
    let truncated = truncate_contract_call_error(error_text.clone());

    let head = truncated
        .strip_suffix(TRUNCATION_MARKER)
        .expect("Truncated text must carry the truncation marker");
    assert!(error_text.starts_with(head), "the kept head must be a prefix of the original");
    assert!(head.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    // Nothing is dropped beyond what the boundary requires.
    assert!(head.len() > MAX_CONTRACT_CALL_ERROR_BYTES - repeated_text.len());
}

/// A reverting view call carries the feed contract's panic data, which the contract sizes.
#[tokio::test]
async fn contract_call_error_text_is_truncated() {
    // The relayed message is the entry point and the feed address, then the capped error text.
    const MAX_ENTRY_POINT_AND_ADDRESS_LENGTH: usize = 128;
    const REASON_LENGTH: usize = 10_000;
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(|_| {
        Err(BatcherClientError::BatcherError(BatcherError::ContractCallFailed {
            reason: "a".repeat(REASON_LENGTH),
        }))
    });
    let client = ChainlinkOracleClient::new(
        ChainlinkRateKind::StrkToUsd,
        test_config(),
        Arc::new(batcher_client),
    );

    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(message)) => {
            assert!(message.ends_with(TRUNCATION_MARKER), "error text was not truncated: {message}");
            assert!(
                message.len()
                    <= MAX_ENTRY_POINT_AND_ADDRESS_LENGTH
                        + MAX_CONTRACT_CALL_ERROR_BYTES
                        + TRUNCATION_MARKER.len(),
                "error text exceeded the cap: {} bytes",
                message.len()
            );
        }
    );
}
