use std::sync::Arc;

use apollo_batcher_types::communication::{BatcherClientError, MockBatcherClient};
use apollo_batcher_types::errors::BatcherError;
use apollo_l1_gas_price_types::CurrencyPair;
use assert_matches::assert_matches;
use rstest::rstest;

use super::*;
use crate::chainlink_oracle::contract_call_error::{
    MAX_CONTRACT_CALL_ERROR_BYTES,
    TRUNCATION_MARKER,
};
use crate::chainlink_oracle::test_utils::{
    batcher_client_from_responses,
    feed_responses,
    FeedFixture,
    FeedResponses,
};

/// The block timestamp every reading below is dated against, and every freshness bound measured
/// against.
const TIMESTAMP: u64 = 1_700_000_000;
/// $0.03 per STRK at `FEED_DECIMALS`.
const STRK_USD_ANSWER: u128 = 3_000_000;

fn test_config() -> ChainlinkOracleConfig {
    ChainlinkOracleConfig::default()
}

fn strk_usd_feed() -> PairFeed {
    test_config().strk_usd_feed(&AllRateBoundsConfig::default())
}

fn fresh_updated_at() -> u64 {
    TIMESTAMP
}

fn stale_updated_at() -> u64 {
    TIMESTAMP - test_config().freshness.max_staleness_seconds - 1
}

fn strk_usd_responses(strk_usd: FeedFixture) -> FeedResponses {
    feed_responses(test_config().strk_usd_feed_address, strk_usd)
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
        Err(ExchangeRateOracleClientError::StaleFeedError { pair, .. })
            if pair == CurrencyPair::StrkUsd
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
            Err(ExchangeRateOracleClientError::FutureFeedError { pair, .. })
                if pair == CurrencyPair::StrkUsd
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
    let bounds_config = AllRateBoundsConfig::default();

    let eth_usd = config.eth_usd_feed(&bounds_config);
    assert_eq!(eth_usd.bounds.pair, CurrencyPair::EthUsd);
    assert_eq!(eth_usd.feed_address, config.eth_usd_feed_address);
    assert_eq!(eth_usd.bounds.minimum_rate, bounds_config.eth_usd_bounds().minimum_rate);
    assert_eq!(eth_usd.freshness.max_staleness_seconds, config.freshness.max_staleness_seconds);
    assert_eq!(
        eth_usd.freshness.max_future_updated_at_seconds,
        config.freshness.max_future_updated_at_seconds
    );

    let strk_usd = config.strk_usd_feed(&bounds_config);
    assert_eq!(strk_usd.bounds.pair, CurrencyPair::StrkUsd);
    assert_eq!(strk_usd.feed_address, config.strk_usd_feed_address);
    assert_eq!(strk_usd.bounds.minimum_rate, bounds_config.strk_usd_bounds().minimum_rate);
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
