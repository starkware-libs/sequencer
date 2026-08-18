use std::sync::Arc;

use apollo_batcher_types::communication::{BatcherClientError, MockBatcherClient};
use apollo_batcher_types::errors::BatcherError;
use apollo_metrics::metrics::{LabeledMetricCounter, MetricDetails};
use assert_matches::assert_matches;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use strum::IntoEnumIterator;

use super::*;
use crate::chainlink_oracle::feed_math::{MAX_CONTRACT_CALL_ERROR_BYTES, TRUNCATION_MARKER};
use crate::chainlink_oracle::test_utils::{
    batcher_client_from_responses,
    decimals_retdata,
    feed_responses,
    FeedFixture,
    FeedResponses,
    FEED_DECIMALS,
};
use crate::metrics::CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT;

/// The block timestamp every reading below is dated against, and every freshness bound measured
/// against.
const TIMESTAMP: u64 = 1_700_000_000;
/// $0.03 per STRK at `FEED_DECIMALS`.
const STRK_USD_ANSWER: u128 = 3_000_000;

fn test_config() -> ChainlinkOracleConfig {
    ChainlinkOracleConfig::default()
}

fn strk_usd_feed() -> FeedRead {
    test_config().strk_usd_feed(&RateBoundsConfig::default())
}

fn fresh_updated_at() -> u64 {
    TIMESTAMP
}

fn stale_updated_at() -> u64 {
    TIMESTAMP - test_config().freshness.max_staleness_seconds - 1
}

fn future_updated_at() -> u64 {
    TIMESTAMP + test_config().freshness.max_future_updated_at_seconds + 1
}

fn strk_usd_responses(strk_usd: FeedFixture) -> FeedResponses {
    feed_responses(test_config().strk_usd_feed_address, strk_usd)
}

/// The STRK/USD feed with a valid `decimals` reply but no round, so exactly one of the two calls a
/// read issues fails.
fn strk_usd_responses_without_round_data() -> FeedResponses {
    FeedResponses::from([(
        (test_config().strk_usd_feed_address, DECIMALS_ENTRY_POINT.to_string()),
        decimals_retdata(FEED_DECIMALS),
    )])
}

/// Reads the STRK/USD feed against `TIMESTAMP`, the timestamp every fixture is dated against.
async fn read_strk_usd(strk_usd: FeedFixture) -> RateResult {
    read_strk_usd_responses(strk_usd_responses(strk_usd)).await
}

async fn read_strk_usd_responses(responses: FeedResponses) -> RateResult {
    read_feed(&batcher_client_from_responses(responses), strk_usd_feed(), TIMESTAMP).await
}

#[tokio::test]
async fn strk_to_usd_rejects_stale_reading() {
    assert_matches!(
        read_strk_usd(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())).await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair_name, .. })
            if pair_name == CurrencyPair::StrkUsd.pair_name()
    );
}

#[tokio::test]
async fn strk_to_usd_accepts_reading_exactly_at_the_staleness_bound() {
    let oldest_accepted_updated_at = TIMESTAMP - test_config().freshness.max_staleness_seconds;
    assert!(
        read_strk_usd(FeedFixture::new(STRK_USD_ANSWER, oldest_accepted_updated_at)).await.is_ok()
    );
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
    let updated_at =
        TIMESTAMP + test_config().freshness.max_future_updated_at_seconds + seconds_past_the_bound;

    let result = read_strk_usd(FeedFixture::new(STRK_USD_ANSWER, updated_at)).await;
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
    assert_matches!(
        read_strk_usd(FeedFixture::new(STRK_USD_ANSWER, u64::MAX)).await,
        Err(ExchangeRateOracleClientError::FutureFeedError { .. })
    );
}

#[tokio::test]
async fn zero_answer_rejected() {
    assert_matches!(
        read_strk_usd(FeedFixture::new(0, fresh_updated_at())).await,
        Err(ExchangeRateOracleClientError::InvalidRateError(message))
            if message.contains("zero answer")
    );
}

// Each accessor names its pair and its feed independently, so what a read is attributed to and
// which feed it reads are pinned here rather than only by the accessor's name.
#[test]
fn feed_accessors_carry_their_own_pair_and_feed() {
    let config = test_config();
    let bounds_config = RateBoundsConfig::default();

    let eth_usd = config.eth_usd_feed(&bounds_config);
    assert_eq!(eth_usd.bounds.pair, CurrencyPair::EthUsd);
    assert_eq!(eth_usd.feed_address, config.eth_usd_feed_address);
    assert_eq!(eth_usd.bounds.minimum_micro_units, bounds_config.eth_usd.minimum_micro_units);
    assert_eq!(eth_usd.freshness.max_staleness_seconds, config.freshness.max_staleness_seconds);
    assert_eq!(
        eth_usd.freshness.max_future_updated_at_seconds,
        config.freshness.max_future_updated_at_seconds
    );

    let strk_usd = config.strk_usd_feed(&bounds_config);
    assert_eq!(strk_usd.bounds.pair, CurrencyPair::StrkUsd);
    assert_eq!(strk_usd.feed_address, config.strk_usd_feed_address);
    assert_eq!(strk_usd.bounds.minimum_micro_units, bounds_config.strk_usd.minimum_micro_units);
}

#[tokio::test]
async fn batcher_call_failure_is_surfaced_as_an_error() {
    assert_matches!(
        read_strk_usd_responses(FeedResponses::new()).await,
        Err(ExchangeRateOracleClientError::ContractCallError(_))
    );
}

/// A reverting view call carries the feed contract's panic data, which the contract sizes.
#[tokio::test]
async fn contract_call_error_text_is_truncated() {
    // The relayed message is the entry point and the feed address, then the capped error text.
    const MAX_ENTRY_POINT_AND_ADDRESS_LENGTH: usize = 128;
    const REASON_LENGTH: usize = 10_000;
    let mut reverting_batcher_client = MockBatcherClient::new();
    reverting_batcher_client.expect_call_contract().returning(|_| {
        Err(BatcherClientError::BatcherError(BatcherError::ContractCallFailed {
            reason: "a".repeat(REASON_LENGTH),
        }))
    });
    let batcher_client: SharedBatcherClient = Arc::new(reverting_batcher_client);

    assert_matches!(
        read_feed(&batcher_client, strk_usd_feed(), TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::ContractCallError(message)) => {
            assert!(
                message.ends_with(TRUNCATION_MARKER),
                "error text was not truncated: {message}"
            );
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

/// The guard counters record why a read was rejected and, through the `currency_pair` label, which
/// reading it was rejected on. Each guard must increment its own counter on the rejected pair's
/// series and on no other.
#[rstest]
#[case::stale_strk_feed(
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())),
    &CHAINLINK_ORACLE_STALE_FEED_COUNT
)]
#[case::future_feed(
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, future_updated_at())),
    &CHAINLINK_ORACLE_FUTURE_FEED_COUNT
)]
#[case::zero_answer(
    strk_usd_responses(FeedFixture::new(0, fresh_updated_at())),
    &CHAINLINK_ORACLE_INVALID_FEED_ANSWER_COUNT
)]
// One micro-cent per STRK, far below the configured floor.
#[case::rate_out_of_bounds(
    strk_usd_responses(FeedFixture::new(1, fresh_updated_at())),
    &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT
)]
#[case::contract_call_failure(
    strk_usd_responses_without_round_data(),
    &CHAINLINK_ORACLE_CONTRACT_CALL_ERROR_COUNT
)]
#[tokio::test]
async fn guard_counters_record_the_rejection_reason_and_pair(
    #[case] responses: FeedResponses,
    #[case] guard_counter: &'static LabeledMetricCounter,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    assert!(read_strk_usd_responses(responses).await.is_err());
    let rendered_metrics = recorder.handle().render();
    for pair in CurrencyPair::iter() {
        // A series only the guard's own increment creates reads as absent, which is the same
        // statement as a count of zero.
        let count = guard_counter
            .parse_numeric_metric::<u64>(&rendered_metrics, &pair.labels())
            .unwrap_or(0);
        let expected_count = u64::from(pair == CurrencyPair::StrkUsd);
        assert_eq!(
            count,
            expected_count,
            "{} on pair {pair:?} recorded {count} rejections, expected {expected_count}",
            guard_counter.get_name()
        );
    }
}
