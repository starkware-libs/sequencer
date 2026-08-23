use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use apollo_config::converters::UrlAndHeaders;
use apollo_l1_gas_price_config::config::{
    ExchangeRateOracleConfig,
    ExchangeRateOracleSource,
    L1GasPriceProviderConfig,
};
use apollo_l1_gas_price_types::{
    CurrencyPair,
    ExchangeRate,
    GasPriceData,
    L1GasPriceProviderResult,
    MockExchangeRateOracleClientTrait,
    PriceInfo,
};
use apollo_metrics::metrics::MetricDetails;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use mockito::{Mock, ServerGuard};
use rstest::rstest;
use serde_json::json;
use starknet_api::block::{BlockTimestamp, GasPrice};
use url::Url;

use crate::chainlink_oracle::test_utils::{
    batcher_client_serving_fresh_feeds,
    ETH_TO_FRI_RATE,
    STRK_TO_USD_RATE,
    TIMESTAMP,
};
use crate::exchange_rate_oracle::EXCHANGE_RATE_DECIMALS;
use crate::l1_gas_price_provider::{L1GasPriceProvider, L1GasPriceProviderError, RingBuffer};
use crate::metrics::EXCHANGE_RATE_ORACLE_RATE;

// One HTTP rate per feed. The four rates the two sources serve are distinct, so a feed served by
// the other feed's config, or by the source it did not select, resolves to a number named for that
// other feed.
const ETH_TO_STRK_HTTP_RATE: ExchangeRate = 111_000;
const STRK_TO_USD_HTTP_RATE: ExchangeRate = 222_000;
// The block timestamp the Chainlink fixtures are dated against, so every rate is queried for it.
const RATE_QUERY_TIMESTAMP: u64 = TIMESTAMP;

// Serves `rate` to every request, in the shape the HTTP oracle expects.
fn mock_rate_response(server: &mut ServerGuard, rate: ExchangeRate) -> Mock {
    server
        .mock("GET", mockito::Matcher::Any)
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({ "price": format!("0x{rate:x}"), "decimals": EXCHANGE_RATE_DECIMALS })
                .to_string(),
        )
        .create()
}

fn http_oracle_config(server_url: &str) -> ExchangeRateOracleConfig {
    ExchangeRateOracleConfig {
        url_header_list: Some(vec![
            UrlAndHeaders {
                url: Url::parse(server_url).expect("The mock server URL should parse"),
                headers: BTreeMap::new(),
            }
            .into(),
        ]),
        ..Default::default()
    }
}

// The first call to either oracle only spawns the query, so the rate is polled until the background
// query lands in the client's cache.
async fn resolve_rate<FetchRate, RateQuery>(fetch_rate: FetchRate) -> ExchangeRate
where
    FetchRate: Fn() -> RateQuery,
    RateQuery: Future<Output = L1GasPriceProviderResult<ExchangeRate>>,
{
    // `sleep` parks the runtime so the IO driver is polled and the HTTP round trip completes;
    // `yield_now` alone does not, since the driver is polled only every few scheduler ticks.
    const POLL_INTERVAL: Duration = Duration::from_millis(10);
    const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
    tokio::time::timeout(RESOLVE_TIMEOUT, async {
        loop {
            if let Ok(rate) = fetch_rate().await {
                return rate;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("The oracle query did not resolve within {RESOLVE_TIMEOUT:?}"))
}

// Make a provider with five block prices. Timestamps are 2 seconds apart, starting from 0.
// To get the prices for the middle three blocks use the timestamp for block[3].
// Returns the provider, a vector of block prices to compare with, and the timestamp of block[3].
fn make_provider() -> (L1GasPriceProvider, Vec<PriceInfo>, u64) {
    let eth_to_strk_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let strk_to_usd_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let mut provider = L1GasPriceProvider::new(
        L1GasPriceProviderConfig { number_of_blocks_for_mean: 3, ..Default::default() },
        eth_to_strk_oracle_client,
        strk_to_usd_oracle_client,
    );
    provider.initialize().unwrap();
    let mut prices = Vec::new();
    let mut timestamp3 = 0;
    for i in 0..5 {
        let block_number = i.try_into().unwrap();
        let price = (i * i).try_into().unwrap();
        let time = (i * 2).try_into().unwrap();
        let price_info =
            PriceInfo { base_fee_per_gas: GasPrice(price), blob_fee: GasPrice(price + 1) };
        prices.push(price_info.clone());
        if i == 3 {
            timestamp3 = time;
        }
        provider
            .add_price_info(GasPriceData {
                block_number,
                timestamp: BlockTimestamp(time),
                price_info,
            })
            .unwrap();
    }
    (provider, prices, timestamp3)
}

#[test]
fn ring_buffer_enforces_configured_limit() {
    const LIMIT: usize = 3;
    const NUM_VALUES_PUSHED: i32 = 100;
    // After pushing 0..NUM_VALUES_PUSHED, only the most recent LIMIT values remain, oldest-first.
    const EXPECTED_REMAINING: [i32; LIMIT] = [97, 98, 99];

    let mut buffer = RingBuffer::new(LIMIT);

    // Push well past the limit. The allocator may round the underlying VecDeque's capacity up
    // above LIMIT, but the buffer must drop the oldest item and never retain more than LIMIT.
    for value in 0..NUM_VALUES_PUSHED {
        buffer.push(value);
        assert!(
            buffer.len() <= LIMIT,
            "buffer retained {} items, exceeding limit {LIMIT}",
            buffer.len()
        );
    }

    let remaining: Vec<_> = buffer.iter().copied().collect();
    assert_eq!(remaining, EXPECTED_REMAINING);
}

#[test]
fn gas_price_provider_mean_prices() {
    let (provider, block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds.as_secs();
    let num_blocks: u128 = provider.config.number_of_blocks_for_mean.into();

    // This calculation will grab config.number_of_blocks_for_mean prices from the middle of the
    // range. timestamp3 (for block_prices[3]) is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // The gas prices should go from block 1 to 3.
    let gas_price_calculation = block_prices[1]
        .base_fee_per_gas
        .saturating_add(block_prices[2].base_fee_per_gas)
        .saturating_add(block_prices[3].base_fee_per_gas)
        .checked_div(num_blocks)
        .expect("Cannot divide by zero");
    let data_price_calculation = block_prices[1]
        .blob_fee
        .saturating_add(block_prices[2].blob_fee)
        .saturating_add(block_prices[3].blob_fee)
        .checked_div(num_blocks)
        .expect("Cannot divide by zero");
    assert_eq!(gas_price, gas_price_calculation);
    assert_eq!(data_gas_price, data_price_calculation);
}

#[test]
fn gas_price_provider_adding_blocks() {
    let (mut provider, _block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds.as_secs();

    // timestamp3 is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // Add a block to the provider.
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(10), blob_fee: GasPrice(11) };
    let timestamp = BlockTimestamp(10);
    provider.add_price_info(GasPriceData { block_number: 5, timestamp, price_info }).unwrap();

    // This should not change the results if we ask for the same timestamp.
    let PriceInfo { base_fee_per_gas: gas_price_new, blob_fee: data_gas_price_new } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    assert_eq!(gas_price, gas_price_new);
    assert_eq!(data_gas_price, data_gas_price_new);

    // Add another block to the provider.
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(12), blob_fee: GasPrice(13) };
    let timestamp = BlockTimestamp(12);
    provider.add_price_info(GasPriceData { block_number: 6, timestamp, price_info }).unwrap();

    // Should fail because the memory of the provider is full, and we added another block.
    let ret = provider.get_price_info(BlockTimestamp(timestamp3 + lag));
    matches!(ret, Result::Err(L1GasPriceProviderError::MissingDataError { .. }));
}

#[test]
fn gas_price_provider_timestamp_changes_mean() {
    let (provider, _block_prices, timestamp3) = make_provider();
    let lag = provider.config.lag_margin_seconds.as_secs();

    // timestamp3 is used to define the interval of blocks 1 to 3.
    let PriceInfo { base_fee_per_gas: gas_price, blob_fee: data_gas_price } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag)).unwrap();

    // If we take a higher timestamp the gas prices should change.
    let PriceInfo { base_fee_per_gas: gas_price_new, blob_fee: data_gas_price_new } =
        provider.get_price_info(BlockTimestamp(timestamp3 + lag * 2)).unwrap();
    assert_ne!(gas_price_new, gas_price);
    assert_ne!(data_gas_price_new, data_gas_price);
}

#[test]
fn gas_price_provider_can_start_at_nonzero_height() {
    let eth_to_strk_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let strk_to_usd_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let mut provider = L1GasPriceProvider::new(
        L1GasPriceProviderConfig { number_of_blocks_for_mean: 3, ..Default::default() },
        eth_to_strk_oracle_client,
        strk_to_usd_oracle_client,
    );
    provider.initialize().unwrap();
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(0), blob_fee: GasPrice(0) };
    let timestamp = BlockTimestamp(0);
    provider.add_price_info(GasPriceData { block_number: 42, timestamp, price_info }).unwrap();
}

#[test]
fn gas_price_provider_uninitialized_error() {
    let eth_to_strk_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let strk_to_usd_oracle_client = Arc::new(MockExchangeRateOracleClientTrait::new());
    let mut provider = L1GasPriceProvider::new(
        L1GasPriceProviderConfig { number_of_blocks_for_mean: 3, ..Default::default() },
        eth_to_strk_oracle_client,
        strk_to_usd_oracle_client,
    );
    let price_info = PriceInfo { base_fee_per_gas: GasPrice(0), blob_fee: GasPrice(0) };
    let timestamp = BlockTimestamp(0);
    let result = provider.add_price_info(GasPriceData { block_number: 42, timestamp, price_info });
    assert!(matches!(result, Err(L1GasPriceProviderError::NotInitializedError)));
}

/// Each feed resolves to the rate its own selected source serves, with both HTTP servers and the
/// batcher wired in every case.
#[rstest]
#[case::both_http(
    ExchangeRateOracleSource::Http,
    ExchangeRateOracleSource::Http,
    ETH_TO_STRK_HTTP_RATE,
    STRK_TO_USD_HTTP_RATE
)]
#[case::eth_to_strk_on_chainlink(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Http,
    ETH_TO_FRI_RATE,
    STRK_TO_USD_HTTP_RATE
)]
#[case::strk_to_usd_on_chainlink(
    ExchangeRateOracleSource::Http,
    ExchangeRateOracleSource::Chainlink,
    ETH_TO_STRK_HTTP_RATE,
    STRK_TO_USD_RATE
)]
#[case::both_chainlink(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Chainlink,
    ETH_TO_FRI_RATE,
    STRK_TO_USD_RATE
)]
#[tokio::test]
async fn each_feed_is_served_by_the_source_selected_for_it(
    #[case] eth_to_strk_oracle_source: ExchangeRateOracleSource,
    #[case] strk_to_usd_oracle_source: ExchangeRateOracleSource,
    #[case] expected_eth_to_fri_rate: ExchangeRate,
    #[case] expected_strk_to_usd_rate: ExchangeRate,
) {
    let mut eth_to_strk_server = mockito::Server::new_async().await;
    let _eth_to_strk_mock = mock_rate_response(&mut eth_to_strk_server, ETH_TO_STRK_HTTP_RATE);
    let mut strk_to_usd_server = mockito::Server::new_async().await;
    let _strk_to_usd_mock = mock_rate_response(&mut strk_to_usd_server, STRK_TO_USD_HTTP_RATE);

    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source,
        strk_to_usd_oracle_source,
        eth_to_strk_oracle_config: http_oracle_config(&eth_to_strk_server.url()),
        strk_to_usd_oracle_config: http_oracle_config(&strk_to_usd_server.url()),
        ..Default::default()
    };
    let provider =
        L1GasPriceProvider::new_with_oracle(config, batcher_client_serving_fresh_feeds());

    assert_eq!(
        resolve_rate(|| provider.eth_to_fri_rate(RATE_QUERY_TIMESTAMP)).await,
        expected_eth_to_fri_rate,
        "The ETH/STRK feed selected {eth_to_strk_oracle_source:?}, so it should have been served \
         by that source, reading the ETH/STRK feed's own config"
    );
    assert_eq!(
        resolve_rate(|| provider.strk_to_usd_rate(RATE_QUERY_TIMESTAMP)).await,
        expected_strk_to_usd_rate,
        "The STRK/USD feed selected {strk_to_usd_oracle_source:?}, so it should have been served \
         by that source, reading the STRK/USD feed's own config"
    );
}

/// Each feed's rate is published on the series labeled with that feed's currency pair, whichever
/// source built the feed's client.
#[rstest]
#[case::http(ExchangeRateOracleSource::Http, ETH_TO_STRK_HTTP_RATE, STRK_TO_USD_HTTP_RATE)]
#[case::chainlink(ExchangeRateOracleSource::Chainlink, ETH_TO_FRI_RATE, STRK_TO_USD_RATE)]
#[tokio::test]
async fn each_feed_publishes_on_its_own_metrics_series(
    #[case] oracle_source: ExchangeRateOracleSource,
    #[case] expected_eth_to_fri_rate: ExchangeRate,
    #[case] expected_strk_to_usd_rate: ExchangeRate,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let mut eth_to_strk_server = mockito::Server::new_async().await;
    let _eth_to_strk_mock = mock_rate_response(&mut eth_to_strk_server, ETH_TO_STRK_HTTP_RATE);
    let mut strk_to_usd_server = mockito::Server::new_async().await;
    let _strk_to_usd_mock = mock_rate_response(&mut strk_to_usd_server, STRK_TO_USD_HTTP_RATE);

    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source: oracle_source,
        strk_to_usd_oracle_source: oracle_source,
        eth_to_strk_oracle_config: http_oracle_config(&eth_to_strk_server.url()),
        strk_to_usd_oracle_config: http_oracle_config(&strk_to_usd_server.url()),
        ..Default::default()
    };
    let provider =
        L1GasPriceProvider::new_with_oracle(config, batcher_client_serving_fresh_feeds());
    resolve_rate(|| provider.eth_to_fri_rate(RATE_QUERY_TIMESTAMP)).await;
    resolve_rate(|| provider.strk_to_usd_rate(RATE_QUERY_TIMESTAMP)).await;

    let rendered_metrics = recorder.handle().render();
    for (pair, expected_rate) in [
        (CurrencyPair::EthStrk, expected_eth_to_fri_rate),
        (CurrencyPair::StrkUsd, expected_strk_to_usd_rate),
    ] {
        assert_eq!(
            EXCHANGE_RATE_ORACLE_RATE
                .parse_numeric_metric::<ExchangeRate>(&rendered_metrics, &pair.labels()),
            Some(expected_rate),
            "The {pair:?} rate the {oracle_source:?} source served was not published on {}",
            EXCHANGE_RATE_ORACLE_RATE.get_name()
        );
    }
}
