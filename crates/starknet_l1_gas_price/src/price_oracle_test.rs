use std::collections::BTreeMap;

use serde_json::json;
use tokio;
use url::Url;

use crate::price_oracle::{PriceOracleClient, PriceOracleClientTrait};

#[tokio::test]
async fn eth_to_fri_rate() {
    let expected_rate = 123456;
    let expected_rate_hex = format!("0x{:x}", expected_rate);

    // Create a mock HTTP server.
    let mut server = mockito::Server::new_async().await;

    // Define a mock response for a GET request with a specific timestamp in the path
    let _m = server
        .mock("GET", "/1234567890")
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

    // Create a dummy headers map
    let mut headers = BTreeMap::new();
    headers.insert("Dummy-header".to_string(), "Dummy-value".to_string());

    let client = PriceOracleClient::new(base_url, headers);
    let rate = client.eth_to_fri_rate(1234567890).await.unwrap();

    assert_eq!(rate, expected_rate);
}
