use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use serde_json::json;
use tokio;
use url::Url;

use crate::eth_to_strk_oracle::EthToStrkOracleClient;

#[tokio::test]
async fn eth_to_fri_rate() {
    let expected_rate = 123456;
    let expected_rate_hex = format!("0x{:x}", expected_rate);
    let timestamp = 1234567890;
    let lag_interval_seconds = 10;
    let quantized_timestamp = (timestamp - lag_interval_seconds) / lag_interval_seconds;
    let adjusted_timestamp = quantized_timestamp * lag_interval_seconds;
    // Create a mock HTTP server.
    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific timestamp in the path
    let _m = server
        .mock("GET", format!("/{}", adjusted_timestamp).as_str())
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();

    // Construct the base URL from the mock server
    let base_url = Url::parse(&server.url()).unwrap();

    let client = EthToStrkOracleClient::new(base_url, None, lag_interval_seconds);
    let rate = client.eth_to_fri_rate(timestamp).await.unwrap();

    assert_eq!(rate, expected_rate);
}

#[tokio::test]
async fn eth_to_fri_rate_uses_cache_on_quantized_hit() {
    let expected_rate = 123456;
    let expected_rate_hex = format!("0x{:x}", expected_rate);
    let timestamp1 = 1234567890;
    let timestamp2 = timestamp1 + 10; // Still in the same quantized bucket
    let lag_interval_seconds = 60;

    let quantized_timestamp = (timestamp1 - lag_interval_seconds) / lag_interval_seconds;
    let adjusted_timestamp = quantized_timestamp * lag_interval_seconds;

    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific adjusted_timestamp in the path
    let _m = server
        .mock("GET", format!("/{}", adjusted_timestamp).as_str())
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();

    let base_url = Url::parse(&server.url()).unwrap();
    let client = EthToStrkOracleClient::new(base_url, None, lag_interval_seconds);

    let rate1 = client.eth_to_fri_rate(timestamp1).await.unwrap();
    // Because caching is used, the second request reuses the first response and doesn't trigger
    // another server call.
    let rate2 = client.eth_to_fri_rate(timestamp2).await.unwrap();

    assert_eq!(rate1, expected_rate);
    assert_eq!(rate2, expected_rate);
}
