use std::sync::atomic::Ordering;

use apollo_l1_gas_price_types::errors::ExchangeRateOracleErrorType;
use apollo_l1_gas_price_types::{CurrencyPair, LABEL_NAME_CURRENCY_PAIR, LABEL_NAME_ERROR_TYPE};
use assert_matches::assert_matches;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use strum::IntoEnumIterator;

use super::*;
use crate::chainlink_oracle::test_utils::{
    batcher_client_failing_after,
    batcher_client_from_responses,
    counting_batcher_client,
    eth_and_strk_responses,
    fresh_updated_at,
    future_updated_at,
    stale_updated_at,
    strk_usd_responses,
    test_config,
    FeedFixture,
    FeedResponses,
    ETH_TO_FRI_RATE,
    ETH_USD_ANSWER,
    STRK_TO_USD_RATE,
    STRK_USD_ANSWER,
    TIMESTAMP,
};
use crate::metrics::EXCHANGE_RATE_ORACLE_ERROR_COUNT;

const MAX_POLL_ATTEMPTS: usize = 1000;
/// `decimals` and `latest_round_data`, per feed.
const CALLS_PER_FEED_PER_QUERY: usize = 2;

fn sampling_interval_seconds() -> u64 {
    test_config().sampling_interval_seconds
}

fn make_client<Kind: ChainlinkRate>(responses: FeedResponses) -> ChainlinkOracleClient<Kind> {
    client_with_batcher(batcher_client_from_responses(responses))
}

fn client_with_batcher<Kind: ChainlinkRate>(
    batcher_client: SharedBatcherClient,
) -> ChainlinkOracleClient<Kind> {
    ChainlinkOracleClient::new(test_config(), AllRateBoundsConfig::default(), batcher_client)
}

/// Polls until the spawned background query resolves, mirroring how consensus retries across
/// proposals.
async fn resolve_rate(
    client: &dyn ExchangeRateOracleClientTrait,
    block_timestamp: u64,
) -> Result<ExchangeRate, ExchangeRateOracleClientError> {
    for _ in 0..MAX_POLL_ATTEMPTS {
        match client.fetch_rate(block_timestamp).await {
            Err(ExchangeRateOracleClientError::QueryNotReadyError(_)) => {
                tokio::task::yield_now().await;
            }
            resolved => return resolved,
        }
    }
    panic!("Query did not resolve within {MAX_POLL_ATTEMPTS} attempts");
}

fn last_valid_read<Kind: ChainlinkRate>(client: &ChainlinkOracleClient<Kind>) -> Option<ValidRead> {
    client.state.lock().unwrap().last_valid_read
}

fn last_attempt_instant<Kind: ChainlinkRate>(
    client: &ChainlinkOracleClient<Kind>,
) -> Option<Instant> {
    client.state.lock().unwrap().last_attempt_instant
}

fn is_query_in_flight<Kind: ChainlinkRate>(client: &ChainlinkOracleClient<Kind>) -> bool {
    client.state.lock().unwrap().query.is_some()
}

/// Waits for the spawned query to finish without calling `fetch_rate`, which would harvest it. This
/// is the state a query is left in when it resolves after the last caller that could have observed
/// it.
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

#[tokio::test]
async fn strk_to_usd_rescales_feed_answer_to_eighteen_decimals() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), STRK_TO_USD_RATE);
}

#[tokio::test]
async fn eth_to_fri_divides_the_two_usd_legs() {
    let updated_at = fresh_updated_at();
    let client = make_client::<EthToFri>(eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, updated_at),
        FeedFixture::new(STRK_USD_ANSWER, updated_at),
    ));
    assert_eq!(resolve_rate(&client, TIMESTAMP).await.unwrap(), ETH_TO_FRI_RATE);
}

/// Each leg is read and checked on its own, so one fresh and one stale leg cannot manufacture a
/// rate. The read is driven through `query_rate`, since the client holds successes only.
#[rstest]
#[case::stale_eth_leg(true, false, CurrencyPair::EthUsd)]
#[case::stale_strk_leg(false, true, CurrencyPair::StrkUsd)]
#[tokio::test]
async fn eth_to_fri_rejects_when_either_leg_is_stale(
    #[case] is_eth_leg_stale: bool,
    #[case] is_strk_leg_stale: bool,
    #[case] expected_pair: CurrencyPair,
) {
    let updated_at =
        |is_stale: bool| if is_stale { stale_updated_at() } else { fresh_updated_at() };
    let batcher_client = batcher_client_from_responses(eth_and_strk_responses(
        FeedFixture::new(ETH_USD_ANSWER, updated_at(is_eth_leg_stale)),
        FeedFixture::new(STRK_USD_ANSWER, updated_at(is_strk_leg_stale)),
    ));

    assert_matches!(
        EthToFri::query_rate(
            &batcher_client,
            &test_config(),
            &AllRateBoundsConfig::default(),
            TIMESTAMP
        )
        .await,
        Err(ExchangeRateOracleClientError::StaleFeedError { pair, .. })
            if pair == expected_pair
    );
}

#[tokio::test]
async fn first_call_spawns_a_query_and_later_calls_are_served_without_requerying() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client = client_with_batcher::<StrkToUsd>(batcher_client);

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

/// `decimals` and `latest_round_data` are read once per sampling interval, however many proposals
/// that interval spans. The call that spawns the refresh must not block on it, so it is served the
/// rate the client already holds.
#[rstest]
#[case::one_second_short(1, false)]
#[case::at_the_interval(0, true)]
#[tokio::test(start_paused = true)]
async fn a_healthy_feed_is_requeried_once_the_sampling_interval_elapses(
    #[case] seconds_short_of_the_interval: u64,
    #[case] is_requeried: bool,
) {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client = client_with_batcher::<StrkToUsd>(batcher_client);
    let rate = resolve_rate(&client, TIMESTAMP).await.unwrap();
    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), CALLS_PER_FEED_PER_QUERY);

    let elapsed_seconds = sampling_interval_seconds() - seconds_short_of_the_interval;
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

/// One query per client at a time: a refresh that comes due while a query is still in flight does
/// not start a second one.
#[tokio::test(start_paused = true)]
async fn no_second_query_starts_while_one_is_in_flight() {
    let (batcher_client, num_batcher_calls) = counting_batcher_client(strk_usd_responses(
        FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at()),
    ));
    let client = client_with_batcher::<StrkToUsd>(batcher_client);
    // A query that never resolves, so the slot is still occupied when the refresh comes due.
    let spawn_instant = Instant::now();
    {
        let mut state = client.state.lock().unwrap();
        state.query = Some(tokio::spawn(std::future::pending()));
        state.last_attempt_instant = Some(spawn_instant);
    }

    tokio::time::advance(Duration::from_secs(sampling_interval_seconds())).await;
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );

    assert_eq!(num_batcher_calls.load(Ordering::SeqCst), 0);
    // A second spawn would have overwritten it.
    assert_eq!(last_attempt_instant(&client), Some(spawn_instant));
}

/// A query can resolve after the last call that could have observed it. That success is still
/// recorded, so the round trip that produced it is not wasted.
#[tokio::test]
async fn a_success_that_resolves_after_its_last_caller_is_not_lost() {
    // Only the first query is served, so the rate below can come from no later one.
    let client = client_with_batcher::<StrkToUsd>(batcher_client_failing_after(
        strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, fresh_updated_at())),
        CALLS_PER_FEED_PER_QUERY,
    ));
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    wait_for_query_to_finish(&client).await;
    assert!(last_valid_read(&client).is_none());

    assert_eq!(client.fetch_rate(TIMESTAMP).await.unwrap(), STRK_TO_USD_RATE);
}

/// A harvested read is dated by the timestamp its own query was issued for, not by the timestamp of
/// the call that harvests it.
#[tokio::test]
async fn a_harvested_success_is_dated_by_its_attempt_timestamp() {
    let client = make_client::<StrkToUsd>(strk_usd_responses(FeedFixture::new(
        STRK_USD_ANSWER,
        fresh_updated_at(),
    )));
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    wait_for_query_to_finish(&client).await;

    let later_timestamp = TIMESTAMP + sampling_interval_seconds();
    assert_eq!(client.fetch_rate(later_timestamp).await.unwrap(), STRK_TO_USD_RATE);
    assert_eq!(
        last_valid_read(&client).expect("The harvested read must be held").block_timestamp,
        TIMESTAMP
    );
}

/// A query the client fails records why it failed and, through the `currency_pair` label, which
/// pair it failed on. Every rejection reason must land on its own `error_type` series and on no
/// other. Recorded at the client rather than at the guards, so every error variant a query can
/// return is counted.
#[rstest]
#[case::stale_feed(
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, stale_updated_at())),
    ExchangeRateOracleErrorType::StaleFeedError
)]
#[case::future_feed(
    strk_usd_responses(FeedFixture::new(STRK_USD_ANSWER, future_updated_at())),
    ExchangeRateOracleErrorType::FutureFeedError
)]
#[case::zero_answer(
    strk_usd_responses(FeedFixture::new(0, fresh_updated_at())),
    ExchangeRateOracleErrorType::InvalidRateError
)]
// One micro-cent per STRK, far below the configured floor.
#[case::rate_out_of_bounds(
    strk_usd_responses(FeedFixture::new(1, fresh_updated_at())),
    ExchangeRateOracleErrorType::RateOutOfBoundsError
)]
#[case::contract_call_failure(FeedResponses::new(), ExchangeRateOracleErrorType::ContractCallError)]
#[tokio::test]
async fn a_failed_query_records_the_rejection_reason_and_pair(
    #[case] responses: FeedResponses,
    #[case] expected_error_type: ExchangeRateOracleErrorType,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let client = make_client::<StrkToUsd>(responses);
    assert_matches!(
        client.fetch_rate(TIMESTAMP).await,
        Err(ExchangeRateOracleClientError::QueryNotReadyError(_))
    );
    wait_for_query_to_finish(&client).await;

    let rendered_metrics = recorder.handle().render();
    for pair in CurrencyPair::iter() {
        for error_type in ExchangeRateOracleErrorType::iter() {
            // A series only this query's own increment creates reads as absent, which is the same
            // statement as a count of zero.
            let count = EXCHANGE_RATE_ORACLE_ERROR_COUNT
                .parse_numeric_metric::<u64>(
                    &rendered_metrics,
                    &[
                        (LABEL_NAME_CURRENCY_PAIR, pair.into()),
                        (LABEL_NAME_ERROR_TYPE, error_type.into()),
                    ],
                )
                .unwrap_or(0);
            let expected_count =
                u64::from(pair == CurrencyPair::StrkUsd && error_type == expected_error_type);
            assert_eq!(
                count, expected_count,
                "{pair:?} / {error_type:?} recorded {count} errors, expected {expected_count}"
            );
        }
    }
}
