use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_config::converters::UrlAndHeaders;
use apollo_l1_gas_price_config::config::{
    ExchangeRateOracleConfig,
    ExchangeRateOracleSource,
    L1GasPriceProviderConfig,
};
use apollo_l1_gas_price_types::{
    CurrencyPair,
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

use crate::exchange_rate_oracle::EXCHANGE_RATE_DECIMALS;
use crate::l1_gas_price_provider::{
    L1GasPriceProvider,
    L1GasPriceProviderError,
    MissingBatcherClientError,
    RingBuffer,
    ETH_TO_STRK_ORACLE_SOURCE_CONFIG_KEY,
    STRK_TO_USD_ORACLE_SOURCE_CONFIG_KEY,
};
use crate::metrics::{ETH_TO_STRK_RATE, SNIP35_STRK_USD_RATE};

const HTTP_CLIENT_TYPE_NAME: &str = "ExchangeRateOracleClient";
const CHAINLINK_CLIENT_TYPE_NAME: &str = "ChainlinkOracleClient";

/// One rate per feed, distinct so that a feed served by the other feed's config, or published on
/// the other feed's metrics, shows up as the wrong number rather than as a passing assertion.
const ETH_TO_STRK_TEST_RATE: u128 = 111_000;
const STRK_TO_USD_TEST_RATE: u128 = 222_000;
const ORACLE_LAG_INTERVAL_SECONDS: u64 = 60;
/// Any timestamp past one lag interval, which the oracle clients quantize before querying.
const RATE_QUERY_TIMESTAMP: u64 = 1_700_000_000;

/// Serves `rate` to every request, in the shape the HTTP oracle expects.
fn mock_rate_response(server: &mut ServerGuard, rate: u128) -> Mock {
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
        lag_interval_seconds: ORACLE_LAG_INTERVAL_SECONDS,
        ..Default::default()
    }
}

/// The first call to an HTTP oracle only spawns the query, so the rate is polled until the
/// background query lands in the client's cache.
async fn resolve_rate<FetchRate, RateQuery>(fetch_rate: FetchRate) -> u128
where
    FetchRate: Fn() -> RateQuery,
    RateQuery: Future<Output = L1GasPriceProviderResult<u128>>,
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

// The clients are held as `Arc<dyn ExchangeRateOracleClientTrait>`, and the concrete type is only
// observable through the `Debug` output the trait requires, which opens with the type's name.
#[rstest]
#[case::both_http(
    ExchangeRateOracleSource::Http,
    ExchangeRateOracleSource::Http,
    HTTP_CLIENT_TYPE_NAME,
    HTTP_CLIENT_TYPE_NAME
)]
#[case::eth_to_strk_only(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Http,
    CHAINLINK_CLIENT_TYPE_NAME,
    HTTP_CLIENT_TYPE_NAME
)]
#[case::strk_to_usd_only(
    ExchangeRateOracleSource::Http,
    ExchangeRateOracleSource::Chainlink,
    HTTP_CLIENT_TYPE_NAME,
    CHAINLINK_CLIENT_TYPE_NAME
)]
#[case::both_chainlink(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Chainlink,
    CHAINLINK_CLIENT_TYPE_NAME,
    CHAINLINK_CLIENT_TYPE_NAME
)]
fn new_with_oracle_builds_the_client_type_selected_for_each_feed(
    #[case] eth_to_strk_oracle_source: ExchangeRateOracleSource,
    #[case] strk_to_usd_oracle_source: ExchangeRateOracleSource,
    #[case] expected_eth_to_strk_client_type: &str,
    #[case] expected_strk_to_usd_client_type: &str,
) {
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source,
        strk_to_usd_oracle_source,
        ..Default::default()
    };
    let provider =
        L1GasPriceProvider::new_with_oracle(config, Some(Arc::new(MockBatcherClient::new())))
            .unwrap();
    let eth_to_strk_client = format!("{:?}", provider.eth_to_strk_oracle_client);
    let strk_to_usd_client = format!("{:?}", provider.strk_to_usd_oracle_client);
    assert!(
        eth_to_strk_client.starts_with(expected_eth_to_strk_client_type),
        "The ETH/STRK feed selected {eth_to_strk_oracle_source:?}, so it should have been served \
         by {expected_eth_to_strk_client_type}, but it was built as: {eth_to_strk_client}"
    );
    assert!(
        strk_to_usd_client.starts_with(expected_strk_to_usd_client_type),
        "The STRK/USD feed selected {strk_to_usd_oracle_source:?}, so it should have been served \
         by {expected_strk_to_usd_client_type}, but it was built as: {strk_to_usd_client}"
    );
}

/// Each HTTP client must be wired to its own feed's config and its own feed's metrics bundle. Both
/// are checked through observed behavior: each feed is served by a URL that returns a rate unique
/// to it, and each rate must surface on the Prometheus series named for that feed.
#[tokio::test]
async fn http_clients_serve_their_own_feed_config_and_metrics() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let mut eth_to_strk_server = mockito::Server::new_async().await;
    let _eth_to_strk_mock = mock_rate_response(&mut eth_to_strk_server, ETH_TO_STRK_TEST_RATE);
    let mut strk_to_usd_server = mockito::Server::new_async().await;
    let _strk_to_usd_mock = mock_rate_response(&mut strk_to_usd_server, STRK_TO_USD_TEST_RATE);

    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source: ExchangeRateOracleSource::Http,
        strk_to_usd_oracle_source: ExchangeRateOracleSource::Http,
        eth_to_strk_oracle_config: http_oracle_config(&eth_to_strk_server.url()),
        strk_to_usd_oracle_config: http_oracle_config(&strk_to_usd_server.url()),
        ..Default::default()
    };
    let provider = L1GasPriceProvider::new_with_oracle(config, None).unwrap();

    assert_eq!(
        resolve_rate(|| provider.eth_to_fri_rate(RATE_QUERY_TIMESTAMP)).await,
        ETH_TO_STRK_TEST_RATE,
        "The ETH/STRK feed was served by the STRK/USD feed's config"
    );
    assert_eq!(
        resolve_rate(|| provider.strk_to_usd_rate(RATE_QUERY_TIMESTAMP)).await,
        STRK_TO_USD_TEST_RATE,
        "The STRK/USD feed was served by the ETH/STRK feed's config"
    );

    let rendered_metrics = recorder.handle().render();
    assert_eq!(
        ETH_TO_STRK_RATE.parse_numeric_metric::<u128>(&rendered_metrics),
        Some(ETH_TO_STRK_TEST_RATE),
        "The ETH/STRK rate was not published on {}",
        ETH_TO_STRK_RATE.get_name()
    );
    assert_eq!(
        SNIP35_STRK_USD_RATE.parse_numeric_metric::<u128>(&rendered_metrics),
        Some(STRK_TO_USD_TEST_RATE),
        "The STRK/USD rate was not published on {}",
        SNIP35_STRK_USD_RATE.get_name()
    );
}

/// A `ChainlinkOracleClient`'s rate kind decides which feeds it reads, which bounds it checks and
/// which metrics bundle it publishes on. It is a type parameter, so each feed's client must be
/// built for the pair that feed serves.
#[test]
fn chainlink_clients_are_built_for_their_own_rate_kind() {
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source: ExchangeRateOracleSource::Chainlink,
        strk_to_usd_oracle_source: ExchangeRateOracleSource::Chainlink,
        ..Default::default()
    };
    let provider =
        L1GasPriceProvider::new_with_oracle(config, Some(Arc::new(MockBatcherClient::new())))
            .unwrap();
    for (feed_name, client, expected_pair) in [
        ("ETH/STRK", &provider.eth_to_strk_oracle_client, CurrencyPair::EthStrk),
        ("STRK/USD", &provider.strk_to_usd_oracle_client, CurrencyPair::StrkUsd),
    ] {
        let debug_output = format!("{client:?}");
        assert!(
            debug_output.contains(&format!("pair: {expected_pair:?}")),
            "The {feed_name} feed should have been built for {expected_pair:?}, but its client \
             is: {debug_output}"
        );
    }
}

/// A Chainlink client samples on the interval of the feed it serves, taken from that feed's own
/// `ExchangeRateOracleConfig`. Switching a feed to Chainlink therefore changes where the rate comes
/// from and not how often it is taken, and one feed's cadence cannot reach the other's client.
#[test]
fn chainlink_clients_sample_on_their_own_feeds_interval() {
    const ETH_TO_STRK_LAG_INTERVAL_SECONDS: u64 = 900;
    const STRK_TO_USD_LAG_INTERVAL_SECONDS: u64 = 300;
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source: ExchangeRateOracleSource::Chainlink,
        strk_to_usd_oracle_source: ExchangeRateOracleSource::Chainlink,
        eth_to_strk_oracle_config: ExchangeRateOracleConfig {
            lag_interval_seconds: ETH_TO_STRK_LAG_INTERVAL_SECONDS,
            ..Default::default()
        },
        strk_to_usd_oracle_config: ExchangeRateOracleConfig {
            lag_interval_seconds: STRK_TO_USD_LAG_INTERVAL_SECONDS,
            ..Default::default()
        },
        ..Default::default()
    };
    let provider =
        L1GasPriceProvider::new_with_oracle(config, Some(Arc::new(MockBatcherClient::new())))
            .unwrap();
    for (feed_name, client, expected_interval_seconds) in [
        ("ETH/STRK", &provider.eth_to_strk_oracle_client, ETH_TO_STRK_LAG_INTERVAL_SECONDS),
        ("STRK/USD", &provider.strk_to_usd_oracle_client, STRK_TO_USD_LAG_INTERVAL_SECONDS),
    ] {
        let debug_output = format!("{client:?}");
        assert!(
            debug_output
                .contains(&format!("sampling_interval_seconds: {expected_interval_seconds}")),
            "The {feed_name} feed should sample every {expected_interval_seconds}s, but its \
             client is: {debug_output}"
        );
    }
}

#[test]
fn http_sources_do_not_need_a_batcher_client() {
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source: ExchangeRateOracleSource::Http,
        strk_to_usd_oracle_source: ExchangeRateOracleSource::Http,
        ..Default::default()
    };
    assert!(L1GasPriceProvider::new_with_oracle(config, None).is_ok());
}

// A Chainlink feed reads through the batcher, so a service wired without a batcher client must be
// rejected while the components are built, not fall back to another source or fail per proposal.
#[rstest]
#[case::eth_to_strk(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Http,
    vec![ETH_TO_STRK_ORACLE_SOURCE_CONFIG_KEY]
)]
#[case::strk_to_usd(
    ExchangeRateOracleSource::Http,
    ExchangeRateOracleSource::Chainlink,
    vec![STRK_TO_USD_ORACLE_SOURCE_CONFIG_KEY]
)]
#[case::both_feeds(
    ExchangeRateOracleSource::Chainlink,
    ExchangeRateOracleSource::Chainlink,
    vec![ETH_TO_STRK_ORACLE_SOURCE_CONFIG_KEY, STRK_TO_USD_ORACLE_SOURCE_CONFIG_KEY]
)]
fn chainlink_source_without_a_batcher_client_is_rejected(
    #[case] eth_to_strk_oracle_source: ExchangeRateOracleSource,
    #[case] strk_to_usd_oracle_source: ExchangeRateOracleSource,
    #[case] expected_source_config_keys: Vec<&'static str>,
) {
    let config = L1GasPriceProviderConfig {
        eth_to_strk_oracle_source,
        strk_to_usd_oracle_source,
        ..Default::default()
    };
    assert_eq!(
        L1GasPriceProvider::new_with_oracle(config, None).unwrap_err(),
        MissingBatcherClientError { source_config_keys: expected_source_config_keys }
    );
}
