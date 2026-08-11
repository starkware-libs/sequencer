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

/// The production sampling cadence, which each feed's `ExchangeRateOracleConfig` carries.
const SAMPLING_INTERVAL_SECONDS: u64 = 900;
const TIMESTAMP: u64 = 1_700_000_000;
const FEED_DECIMALS: u32 = 8;
const MAX_POLL_ATTEMPTS: usize = 1000;
const MICRO_UNITS_PER_UNIT: u64 = 1_000_000;
/// `decimals` and `latest_round_data`, per feed.
const CALLS_PER_FEED_PER_QUERY: usize = 2;

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

fn sampling_interval() -> NonZeroU64 {
    NonZeroU64::new(SAMPLING_INTERVAL_SECONDS).expect("Sampling interval must be non-zero")
}

/// The readings below are dated against `TIMESTAMP`, which every freshness bound is measured
/// against.
fn fresh_updated_at() -> u64 {
    TIMESTAMP
}

fn stale_updated_at() -> u64 {
    TIMESTAMP - test_config().max_staleness_seconds - 1
}

fn future_updated_at() -> u64 {
    TIMESTAMP + test_config().max_future_updated_at_seconds + 1
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
    feed_responses(test_config().strk_usd.feed_address, strk_usd)
}

/// The STRK/USD feed with a valid `decimals` reply but no round, so exactly one of the two calls
/// the client issues per feed fails.
fn strk_usd_responses_without_round_data() -> FeedResponses {
    HashMap::from([(
        (test_config().strk_usd.feed_address, DECIMALS_ENTRY_POINT.to_string()),
        decimals_retdata(FEED_DECIMALS),
    )])
}

fn eth_and_strk_responses(eth_usd: FeedFixture, strk_usd: FeedFixture) -> FeedResponses {
    let mut responses = feed_responses(test_config().eth_usd.feed_address, eth_usd);
    responses.extend(feed_responses(test_config().strk_usd.feed_address, strk_usd));
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
/// which lets a test hold a successful read followed by a failing one.
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

fn make_client<Kind: ChainlinkRate>(responses: FeedResponses) -> ChainlinkOracleClient<Kind> {
    ChainlinkOracleClient::new(
        test_config(),
        sampling_interval(),
        batcher_client_from_responses(responses),
    )
}

/// A client behind the trait object, so that a test case can carry the rate kind as a factory
/// rather than as a value.
fn dyn_client<Kind: ChainlinkRate>(
    responses: FeedResponses,
) -> Arc<dyn ExchangeRateOracleClientTrait> {
    Arc::new(make_client::<Kind>(responses))
}

/// Polls until the spawned background query resolves, mirroring how consensus retries across
/// proposals.
async fn resolve_rate(
    client: &dyn ExchangeRateOracleClientTrait,
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

fn last_valid_rate<Kind: ChainlinkRate>(client: &ChainlinkOracleClient<Kind>) -> Option<u128> {
    client.state.lock().unwrap().last_valid_read.map(|valid_read| valid_read.rate)
}

/// The failure the client holds, which a valid read close enough to the caller masks.
fn held_error<Kind: ChainlinkRate>(
    client: &ChainlinkOracleClient<Kind>,
) -> Option<ExchangeRateOracleClientError> {
    client.state.lock().unwrap().last_error.clone()
}

fn last_attempt_instant<Kind: ChainlinkRate>(
    client: &ChainlinkOracleClient<Kind>,
) -> Option<Instant> {
    client.state.lock().unwrap().last_attempt_instant
}

fn is_query_in_flight<Kind: ChainlinkRate>(client: &ChainlinkOracleClient<Kind>) -> bool {
    client.state.lock().unwrap().query.is_some()
}

/// Drives `fetch_rate` until the client holds a failure. Unlike `resolve_rate` this does not stop
/// at the first `Ok`, which for a failing query is the last valid rate.
async fn wait_for_held_error<Kind: ChainlinkRate>(
    client: &ChainlinkOracleClient<Kind>,
    timestamp: u64,
) {
    for _ in 0..MAX_POLL_ATTEMPTS {
        if held_error(client).is_some() {
            return;
        }
        let _ = client.fetch_rate(timestamp).await;
        tokio::task::yield_now().await;
    }
    panic!("Query for timestamp {timestamp} did not fail within {MAX_POLL_ATTEMPTS} attempts");
}

/// Waits for the spawned query to finish without calling `fetch_rate`, which would harvest it.
/// This is the state a query is left in when it resolves after the last caller that could have
/// observed it.
async fn wait_for_query_to_finish<Kind: ChainlinkRate>(client: &ChainlinkOracleClient<Kind>) {
    for _ in 0..MAX_POLL_ATTEMPTS {
        let is_finished = {
            let state = client.state.lock().unwrap();
            state.query.as_ref().is_some_and(|query| query.is_finished())
        };
        if is_finished {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("Query did not finish within {MAX_POLL_ATTEMPTS} attempts");
}

fn micro_units_to_answer(micro_units: u64, feed_decimals: u32) -> u128 {
    u128::from(micro_units) * 10u128.pow(feed_decimals - MICRO_UNIT_DECIMALS)
}

fn micro_units_to_rate(micro_units: u64) -> u128 {
    u128::from(micro_units) * MICRO_UNIT_TO_RATE_SCALE
}

#[tokio::test]
async fn strk_to_usd_rescales_feed_answer_to_eighteen_decimals() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    // $0.03 per STRK.
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), STRK_TO_USD_RATE);
}

#[tokio::test]
async fn eth_to_fri_divides_the_two_usd_legs() {
    let updated_at = fresh_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, updated_at),
        FeedFixture::new(STRK_USD_ANSWER, updated_at),
    ));
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
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(3000 * 10u128.pow(eth_usd_decimals), updated_at)
            .with_decimals(eth_usd_decimals),
        FeedFixture::new(3 * 10u128.pow(strk_usd_decimals) / 100, updated_at)
            .with_decimals(strk_usd_decimals),
    ));
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
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, updated_at),
        FeedFixture::new(STRK_USD_ANSWER_SEVEN_CENTS, updated_at),
    ));
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), EXPECTED_RATE);
}

#[tokio::test]
async fn strk_to_usd_rejects_stale_reading() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        stale_updated_at(),
    )));
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair_name, .. })
            if pair_name == CurrencyPair::StrkUsd.pair_name()
    );
}

#[tokio::test]
async fn strk_to_usd_accepts_reading_exactly_at_the_staleness_bound() {
    let config = test_config();
    let oldest_accepted_updated_at = TIMESTAMP - config.max_staleness_seconds;
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        oldest_accepted_updated_at,
    )));
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
    let updated_at = TIMESTAMP + config.max_future_updated_at_seconds + seconds_past_the_bound;
    let client =
        make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, updated_at)));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::FutureFeedError { pair_name, .. })
                if pair_name == CurrencyPair::StrkUsd.pair_name()
        );
    }
}

/// `u64::MAX` as `updated_at` is rejected by the future bound (see `read_feed`).
#[tokio::test]
async fn maximal_future_updated_at_rejected() {
    let client =
        make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, u64::MAX)));
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::FutureFeedError { .. })
    );
}

/// A fresh leg divided by a stale leg produces a phantom rate move, so either stale leg must
/// reject the whole derived rate.
#[rstest]
#[case::stale_eth_leg(true, false, CurrencyPair::EthUsd)]
#[case::stale_strk_leg(false, true, CurrencyPair::StrkUsd)]
#[tokio::test]
async fn eth_to_fri_rejects_when_either_leg_is_stale(
    #[case] is_eth_leg_stale: bool,
    #[case] is_strk_leg_stale: bool,
    #[case] expected_pair: CurrencyPair,
) {
    let fresh_timestamp = fresh_updated_at();
    let stale_timestamp = stale_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(
            ETH_USD_ANSWER,
            if is_eth_leg_stale { stale_timestamp } else { fresh_timestamp },
        ),
        FeedFixture::new(
            STRK_USD_ANSWER,
            if is_strk_leg_stale { stale_timestamp } else { fresh_timestamp },
        ),
    ));
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair_name, .. })
            if pair_name == expected_pair.pair_name()
    );
}

#[tokio::test]
async fn zero_answer_rejected() {
    let client =
        make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(0, fresh_updated_at())));
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
    let client = make_client::<StrkToUsd>(strk_usd_responses(
        FeedFixture::new(answer, fresh_updated_at()).with_decimals(feed_decimals),
    ));

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
    let strk_usd_feed_address = test_config().strk_usd.feed_address;
    let responses = HashMap::from([
        ((strk_usd_feed_address, DECIMALS_ENTRY_POINT.to_string()), vec![Felt::MAX]),
        (
            (strk_usd_feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            round_retdata(STRK_USD_ANSWER, fresh_updated_at()),
        ),
    ]);
    let client = make_client::<StrkToUsd>(responses);
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
    let boundary_answer = micro_units_to_answer(config.strk_usd.minimum_micro_units, FEED_DECIMALS);
    let answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    let client =
        make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(answer, fresh_updated_at())));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(result.unwrap(), micro_units_to_rate(config.strk_usd.minimum_micro_units));
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == CurrencyPair::StrkUsd.pair_name()
        );
    }
}

#[rstest]
#[case::at_maximum(0, true)]
#[case::just_above_maximum(1, false)]
#[tokio::test]
async fn strk_usd_maximum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer = micro_units_to_answer(config.strk_usd.maximum_micro_units, FEED_DECIMALS);
    let answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    let client =
        make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(answer, fresh_updated_at())));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(result.unwrap(), micro_units_to_rate(config.strk_usd.maximum_micro_units));
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == CurrencyPair::StrkUsd.pair_name()
        );
    }
}

#[rstest]
#[case::at_minimum(0, true)]
#[case::just_below_minimum(-1, false)]
#[tokio::test]
async fn eth_usd_minimum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer = micro_units_to_answer(config.eth_usd.minimum_micro_units, FEED_DECIMALS);
    let eth_usd_answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    // $0.001 per STRK keeps the derived rate at 20,000 STRK per ETH, inside its own bounds, so
    // only the ETH/USD bound can trip.
    const STRK_USD_ANSWER_TENTH_OF_A_CENT: u128 = 100_000;
    let updated_at = fresh_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(eth_usd_answer, updated_at),
        FeedFixture::new(STRK_USD_ANSWER_TENTH_OF_A_CENT, updated_at),
    ));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == CurrencyPair::EthUsd.pair_name()
        );
    }
}

#[rstest]
#[case::at_maximum(0, true)]
#[case::just_above_maximum(1, false)]
#[tokio::test]
async fn eth_usd_maximum_bound_is_inclusive(#[case] answer_offset: i8, #[case] is_accepted: bool) {
    let config = test_config();
    let boundary_answer = micro_units_to_answer(config.eth_usd.maximum_micro_units, FEED_DECIMALS);
    let eth_usd_answer = boundary_answer.wrapping_add_signed(answer_offset.into());
    // $0.50 per STRK keeps the derived rate at 100,000 STRK per ETH, inside its own bounds.
    const STRK_USD_ANSWER_FIFTY_CENTS: u128 = 50_000_000;
    let updated_at = fresh_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(eth_usd_answer, updated_at),
        FeedFixture::new(STRK_USD_ANSWER_FIFTY_CENTS, updated_at),
    ));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert!(result.is_ok());
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == CurrencyPair::EthUsd.pair_name()
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
        DerivedRateBound::Minimum => config.eth_to_fri.minimum_micro_units,
        DerivedRateBound::Maximum => config.eth_to_fri.maximum_micro_units,
    };
    let strk_usd_answer = strk_usd_answer_for_derived_rate(boundary_rate_micro_strk)
        .wrapping_add_signed(strk_usd_answer_offset.into());
    let updated_at = fresh_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(
            micro_units_to_answer(ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD, FEED_DECIMALS),
            updated_at,
        ),
        FeedFixture::new(strk_usd_answer, updated_at),
    ));

    let result = resolve_rate(&client, TIMESTAMP).await;
    if is_accepted {
        assert_eq!(result.unwrap(), micro_units_to_rate(boundary_rate_micro_strk));
    } else {
        assert_matches!(
            result,
            Err(ExchangeRateOracleClientError::RateOutOfBoundsError { pair_name, .. })
                if pair_name == CurrencyPair::EthStrk.pair_name()
        );
    }
}

#[tokio::test]
async fn extreme_answer_errors_instead_of_overflowing() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        u128::MAX,
        fresh_updated_at(),
    )));
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
    let strk_usd_feed_address = test_config().strk_usd.feed_address;
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
    let client = make_client::<StrkToUsd>(responses);
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ParseError(_))
    );
}

#[tokio::test]
async fn first_call_spawns_a_query_and_later_calls_are_served_without_requerying() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);

    // The batcher round trip must never block the proposal path.
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );

    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);

    const NUM_LATER_CALLS: usize = 10;
    for _ in 0..NUM_LATER_CALLS {
        assert_eq!(client.fetch_rate(TIMESTAMP).await.unwrap(), rate);
    }
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);
}

/// The call that spawns the refresh must not block on it, so it is served the rate the client
/// already holds.
#[tokio::test(start_paused = true)]
async fn a_call_that_spawns_a_query_is_served_the_last_valid_rate() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    tokio::time::advance(Duration::from_secs(SAMPLING_INTERVAL_SECONDS)).await;
    let later_timestamp = TIMESTAMP + SAMPLING_INTERVAL_SECONDS;
    assert_eq!(client.fetch_rate(later_timestamp).await.unwrap(), rate);
    assert!(is_query_in_flight(&client), "the refresh was due but no query was spawned");
}

/// A held failure keeps the batcher from being queried again before the retry interval, but it must
/// not deny the proposal path a rate the client already holds.
#[tokio::test(start_paused = true)]
async fn a_held_failure_does_not_mask_the_last_valid_rate() {
    let client = ChainlinkOracleClient::<StrkToUsd>::new(
        test_config(),
        sampling_interval(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_QUERY,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    tokio::time::advance(Duration::from_secs(SAMPLING_INTERVAL_SECONDS)).await;
    let later_timestamp = TIMESTAMP + SAMPLING_INTERVAL_SECONDS;
    wait_for_held_error(&client, later_timestamp).await;

    assert_eq!(client.fetch_rate(later_timestamp).await.unwrap(), rate);
}

/// The fallback must hold for the one call that observes a failing query finish and stores the
/// failure, not just the calls before and after it.
#[tokio::test(start_paused = true)]
async fn no_call_is_denied_a_rate_the_client_holds() {
    let client = ChainlinkOracleClient::<StrkToUsd>::new(
        test_config(),
        sampling_interval(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_QUERY,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    tokio::time::advance(Duration::from_secs(SAMPLING_INTERVAL_SECONDS)).await;
    let later_timestamp = TIMESTAMP + SAMPLING_INTERVAL_SECONDS;
    for _ in 0..MAX_POLL_ATTEMPTS {
        if held_error(&client).is_some() {
            break;
        }
        assert_eq!(
            client.fetch_rate(later_timestamp).await.expect("Last valid rate should be served"),
            rate
        );
        tokio::task::yield_now().await;
    }
    assert!(held_error(&client).is_some(), "the failing query was never harvested");
}

/// The retry interval governs failed reads only. A successful read is served for the whole sampling
/// interval, however many retry intervals it spans.
#[tokio::test(start_paused = true)]
async fn a_successful_read_is_not_requeried_before_the_sampling_interval() {
    let failure_retry_interval_seconds = test_config().failure_retry_interval_seconds;
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    let num_retry_intervals_before_the_refresh =
        (SAMPLING_INTERVAL_SECONDS - 1) / failure_retry_interval_seconds;
    assert!(num_retry_intervals_before_the_refresh > 1);
    for num_elapsed_intervals in 1..=num_retry_intervals_before_the_refresh {
        tokio::time::advance(Duration::from_secs(failure_retry_interval_seconds)).await;
        let later_timestamp = TIMESTAMP + num_elapsed_intervals * failure_retry_interval_seconds;
        assert_eq!(client.fetch_rate(later_timestamp).await.unwrap(), rate);
    }
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);
}

/// `decimals` and `latest_round_data` are read once per sampling interval, however many proposals
/// that interval spans.
#[rstest]
#[case::one_second_short(SAMPLING_INTERVAL_SECONDS - 1, false)]
#[case::at_the_interval(SAMPLING_INTERVAL_SECONDS, true)]
#[tokio::test(start_paused = true)]
async fn a_healthy_feed_is_requeried_once_the_sampling_interval_elapses(
    #[case] elapsed_seconds: u64,
    #[case] is_requeried: bool,
) {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);

    tokio::time::advance(Duration::from_secs(elapsed_seconds)).await;
    assert_eq!(client.fetch_rate(TIMESTAMP + elapsed_seconds).await.unwrap(), rate);

    assert_eq!(is_query_in_flight(&client), is_requeried);
    if is_requeried {
        wait_for_query_to_finish(&client).await;
        assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 2 * CALLS_PER_FEED_PER_QUERY);
    } else {
        assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);
    }
}

#[tokio::test]
async fn batcher_call_failure_is_surfaced_as_an_error() {
    let client = make_client::<StrkToUsd>(FeedResponses::new());
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
}

/// With no valid rate to fall back on, the caller sees the failure, and the failing feed is
/// queried once per retry interval rather than once per call.
#[tokio::test(start_paused = true)]
async fn failed_query_is_held_until_the_retry_interval_elapses() {
    let failure_retry_interval_seconds = test_config().failure_retry_interval_seconds;
    let (batcher_client, num_batcher_calls) = counting_batcher_client(FeedResponses::new());
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);

    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
    let num_calls_after_failure = num_batcher_calls.load(Ordering::SeqCst);
    assert_eq!(last_valid_rate(&client), None);

    // Every step together stays one second short of the retry interval.
    const NUM_LATER_CALLS: u64 = 10;
    let step_seconds = (failure_retry_interval_seconds - 1) / NUM_LATER_CALLS;
    assert!(step_seconds > 0);
    for _ in 0..NUM_LATER_CALLS {
        tokio::time::advance(Duration::from_secs(step_seconds)).await;
        assert_matches!(
            client.fetch_rate(TIMESTAMP).await,
            Err(ExchangeRateOracleClientError::ContractCallError(_))
        );
    }
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), num_calls_after_failure);
}

/// The freshness guards are measured against the block timestamp the caller asks for, so a round
/// written in that very block is accepted however far it leads the timestamp of the client's
/// previous read.
#[tokio::test]
async fn freshness_is_measured_against_the_block_timestamp() {
    let config = test_config();
    let offset_into_the_interval = config.max_future_updated_at_seconds + 1;
    assert!(offset_into_the_interval < SAMPLING_INTERVAL_SECONDS);
    let block_timestamp = TIMESTAMP + offset_into_the_interval;

    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        block_timestamp,
    )));
    assert_eq!(resolve_rate(&client, block_timestamp).await.unwrap(), STRK_TO_USD_RATE);
}

/// A failed read is retried a retry interval later, so a transient failure costs one retry interval
/// rather than the rest of the sampling interval.
#[tokio::test(start_paused = true)]
async fn a_failed_read_is_retried_after_the_retry_interval() {
    let failure_retry_interval_seconds = test_config().failure_retry_interval_seconds;
    let (batcher_client, num_batcher_calls) = counting_batcher_client(FeedResponses::new());
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);
    assert_matches!(
        resolve_rate(&client, TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
    let num_calls_after_first_attempt = num_batcher_calls.load(Ordering::SeqCst);

    // One second short of the retry interval, the failure still stands and nothing is queried.
    tokio::time::advance(Duration::from_secs(failure_retry_interval_seconds - 1)).await;
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), num_calls_after_first_attempt);

    // At the retry interval the feed is queried again. The held failure is what this call is
    // served, since the retry it spawns has nothing to answer with yet.
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
    assert!(is_query_in_flight(&client), "the retry interval elapsed but no query was spawned");
    wait_for_query_to_finish(&client).await;
    assert!(
        num_batcher_calls.load(Ordering::SeqCst) > num_calls_after_first_attempt,
        "the retry issued no batcher call"
    );
}

/// A query can resolve after the last call that could have observed it. That success must still be
/// recorded, or the round trip is wasted and the fallback ages from an older read than the client
/// actually obtained.
#[tokio::test]
async fn a_success_that_resolves_after_its_last_caller_is_not_lost() {
    let client = ChainlinkOracleClient::<StrkToUsd>::new(
        test_config(),
        sampling_interval(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_QUERY,
        ),
    );
    // The only call that could observe this query, so nothing harvests it here.
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    wait_for_query_to_finish(&client).await;
    assert_eq!(last_valid_rate(&client), None);

    assert_eq!(client.fetch_rate(TIMESTAMP).await.unwrap(), STRK_TO_USD_RATE);
}

/// A re-proposal asks for an earlier timestamp than the read the client resolved. It is served that
/// read rather than being told the query is not ready.
#[tokio::test]
async fn a_resolved_success_is_served_to_an_earlier_timestamp() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();
    let num_calls_after_resolution = num_batcher_calls.load(Ordering::SeqCst);

    let earlier_timestamp = TIMESTAMP - SAMPLING_INTERVAL_SECONDS;
    assert_eq!(client.fetch_rate(earlier_timestamp).await.unwrap(), rate);
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), num_calls_after_resolution);
}

/// The last valid read is served while it is within `MAX_FALLBACK_SAMPLING_INTERVALS` of the block
/// timestamp asked for, however many intervals passed without a call.
#[rstest]
#[case::at_the_allowance(MAX_FALLBACK_SAMPLING_INTERVALS, true)]
#[case::one_interval_past_the_allowance(MAX_FALLBACK_SAMPLING_INTERVALS + 1, false)]
#[case::many_intervals_later(60, false)]
#[tokio::test(start_paused = true)]
async fn last_valid_rate_is_served_only_within_the_allowance(
    #[case] num_intervals_ahead: u64,
    #[case] is_served: bool,
) {
    let client = ChainlinkOracleClient::<StrkToUsd>::new(
        test_config(),
        sampling_interval(),
        batcher_client_failing_after(
            strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
            CALLS_PER_FEED_PER_QUERY,
        ),
    );
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    // No call is made in between, so nothing refreshes the read the client holds.
    let elapsed_seconds = num_intervals_ahead * SAMPLING_INTERVAL_SECONDS;
    tokio::time::advance(Duration::from_secs(elapsed_seconds)).await;
    let later_timestamp = TIMESTAMP + elapsed_seconds;
    wait_for_held_error(&client, later_timestamp).await;

    let result = client.fetch_rate(later_timestamp).await;
    if is_served {
        assert_eq!(result.unwrap(), rate);
    } else {
        assert_matches!(result, Err(ExchangeRateOracleClientError::ContractCallError(_)));
    }
}

/// A re-proposal asks for a timestamp the client has already read past, so it is served a read that
/// leads it.
#[tokio::test]
async fn an_earlier_timestamp_is_served_a_held_rate_within_the_allowance() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();

    let earlier_timestamp = TIMESTAMP - MAX_FALLBACK_SAMPLING_INTERVALS * SAMPLING_INTERVAL_SECONDS;
    assert_eq!(client.fetch_rate(earlier_timestamp).await.unwrap(), rate);
}

/// The allowance bounds the distance in both directions, so a timestamp further back than it is not
/// priced from a read taken that far ahead of it.
#[tokio::test]
async fn an_earlier_timestamp_past_the_allowance_is_not_served_a_held_rate() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    resolve_rate(&client, TIMESTAMP).await.unwrap();

    let earlier_timestamp =
        TIMESTAMP - (MAX_FALLBACK_SAMPLING_INTERVALS + 1) * SAMPLING_INTERVAL_SECONDS;
    assert_matches!(
        client.fetch_rate(earlier_timestamp).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
}

/// One query per client at a time: a refresh that comes due while a query is still in flight does
/// not start a second one.
#[tokio::test(start_paused = true)]
async fn no_second_query_starts_while_one_is_in_flight() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client =
        ChainlinkOracleClient::<StrkToUsd>::new(test_config(), sampling_interval(), batcher_client);
    // A query that never resolves, so the slot is still occupied when the refresh comes due.
    let spawn_instant = Instant::now();
    {
        let mut state = client.state.lock().unwrap();
        state.query = Some(AbortOnDropHandle::new(tokio::spawn(std::future::pending())));
        state.last_attempt_instant = Some(spawn_instant);
    }

    tokio::time::advance(Duration::from_secs(SAMPLING_INTERVAL_SECONDS)).await;
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );

    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 0);
    // A second spawn would have overwritten it.
    assert_eq!(last_attempt_instant(&client), Some(spawn_instant));
}

/// A harvested read is dated by the timestamp its own query was issued for, not by the timestamp of
/// the call that harvests it, so its distance from later callers keeps growing.
#[tokio::test]
async fn a_harvested_success_is_dated_by_its_attempt_timestamp() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    // The only call for this timestamp, so the query resolves unharvested.
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    wait_for_query_to_finish(&client).await;

    let far_later_timestamp =
        TIMESTAMP + (MAX_FALLBACK_SAMPLING_INTERVALS + 1) * SAMPLING_INTERVAL_SECONDS;
    assert_matches!(
        client.fetch_rate(far_later_timestamp).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
}

/// Both legs stay inside their own bands; only the derived rate crosses its floor. The rate moves
/// inversely with the STRK price, so raising that price by one unit is what breaks the floor.
fn responses_with_derived_rate_below_floor() -> FeedResponses {
    let updated_at = fresh_updated_at();
    let strk_usd_answer =
        strk_usd_answer_for_derived_rate(test_config().eth_to_fri.minimum_micro_units) + 1;
    eth_and_strk_responses(
        FeedFixture::new(
            micro_units_to_answer(ETH_USD_PRICE_FOR_DERIVED_BOUNDS_MICRO_USD, FEED_DECIMALS),
            updated_at,
        ),
        FeedFixture::new(strk_usd_answer, updated_at),
    )
}

/// The guard counters record why a query was rejected and, through the `currency_pair` label, which
/// reading it was rejected on. Each guard must increment its own counter on the rejected pair's
/// series and on no other, so that a stale ETH/USD leg is distinguishable from a stale STRK/USD
/// leg.
///
/// The client is built by the case's factory rather than passed in, so that it is constructed after
/// the local recorder is installed.
#[rstest]
#[case::stale_strk_feed(
    dyn_client::<StrkToUsd>,
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    CurrencyPair::StrkUsd
)]
#[case::stale_eth_leg(
    dyn_client::<EthToFri>,
    eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, stale_updated_at()),
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    CurrencyPair::EthUsd
)]
#[case::stale_strk_leg(
    dyn_client::<EthToFri>,
    eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, fresh_updated_at()),
        FeedFixture::new(STRK_USD_ANSWER, stale_updated_at()),
    ),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT,
    CurrencyPair::StrkUsd
)]
#[case::future_feed(
    dyn_client::<StrkToUsd>,
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, future_updated_at())),
    &CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    CurrencyPair::StrkUsd
)]
#[case::zero_answer(
    dyn_client::<StrkToUsd>,
    strk_usd_responses(FeedFixture::new(0, fresh_updated_at())),
    &CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT,
    CurrencyPair::StrkUsd
)]
// One micro-cent per STRK, far below the configured floor.
#[case::rate_out_of_bounds(
    dyn_client::<StrkToUsd>,
    strk_usd_responses(FeedFixture::new(1, fresh_updated_at())),
    &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CurrencyPair::StrkUsd
)]
#[case::derived_rate_out_of_bounds(
    dyn_client::<EthToFri>,
    responses_with_derived_rate_below_floor(),
    &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CurrencyPair::EthStrk
)]
#[case::contract_call_failure(
    dyn_client::<StrkToUsd>,
    strk_usd_responses_without_round_data(),
    &CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT,
    CurrencyPair::StrkUsd
)]
#[tokio::test]
async fn guard_counters_record_the_rejection_reason_and_pair(
    #[case] build_client: fn(FeedResponses) -> Arc<dyn ExchangeRateOracleClientTrait>,
    #[case] responses: FeedResponses,
    #[case] guard_counter: &'static LabeledMetricCounter,
    #[case] rejected_pair: CurrencyPair,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    let client = build_client(responses);

    assert!(resolve_rate(client.as_ref(), TIMESTAMP).await.is_err());
    let rendered_metrics = recorder.handle().render();
    for pair in CurrencyPair::iter() {
        // A series only the guard's own increment creates reads as absent, which is the same
        // statement as a count of zero.
        let count = guard_counter
            .parse_numeric_metric::<u64>(&rendered_metrics, &pair.labels())
            .unwrap_or(0);
        let expected_count = u64::from(pair == rejected_pair);
        assert_eq!(
            count,
            expected_count,
            "{} on pair {pair:?} recorded {count} rejections, expected {expected_count}",
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
    let client = ChainlinkOracleClient::<StrkToUsd>::new(
        test_config(),
        sampling_interval(),
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
