use std::collections::BTreeMap;

use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use serde_json::json;
use tokio::{self};
use url::Url;

use crate::eth_to_strk_oracle::{EthToStrkOracleClient, EthToStrkOracleConfig, UrlAndHeaders};

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
        .mock("GET", "/") // Match the base path only.
        .match_query(mockito::Matcher::UrlEncoded("timestamp".into(), adjusted_timestamp.to_string()))
        .with_header("Content-Type", "application/json")
        .with_body(
            json!({
                "price": expected_rate_hex,
                "decimals": 18
            })
            .to_string(),
        )
        .create();
    let url_and_headers = UrlAndHeaders {
        url: Url::parse(&server.url()).unwrap(),
        headers: BTreeMap::new(), // No additional headers needed for this test.
    };
    let url_header_list = Some(vec![url_and_headers]);
    let config =
        EthToStrkOracleConfig { url_header_list, lag_interval_seconds, ..Default::default() };
    let client = EthToStrkOracleClient::new(config.clone());

    // First request should fail because the cache is empty.
    assert!(client.eth_to_fri_rate(timestamp1).await.is_err());
    // Wait for the query to resolve.
    while client.eth_to_fri_rate(timestamp1).await.is_err() {
        tokio::task::yield_now().await; // Don't block the executor.
    }
    let rate1 = client.eth_to_fri_rate(timestamp1).await.unwrap();
    let rate2 = client
        .eth_to_fri_rate(timestamp2)
        .await
        .expect("Should resolve immediately due to the cache");
    assert_eq!(rate1, rate2);
}
